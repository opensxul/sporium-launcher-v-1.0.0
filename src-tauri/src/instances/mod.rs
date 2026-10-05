mod collections;
pub(crate) mod filesystem;
pub mod icons;
mod journal;
pub mod model;
pub(crate) mod repository;
pub mod summary;

use crate::{error::CoreError, storage::Database};
use filesystem::{INSTANCE_DIRECTORIES, Paths};
use journal::{Pending, Phase};
use model::*;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Library {
    root: PathBuf,
    database: Database,
}

impl Library {
    pub fn new(root: PathBuf, database: Database) -> Self {
        Self { root, database }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn access<T>(
        &self,
        action: impl FnOnce(&mut Connection, &Paths) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let paths = Paths::new(&self.root)?;
        let _lock = paths.lock()?;
        if self.database.path() != self.root.join("launcher/sporium.sqlite3") {
            return Err(CoreError::UnsafePath);
        }
        for name in [
            "sporium.sqlite3",
            "sporium.sqlite3-wal",
            "sporium.sqlite3-shm",
        ] {
            paths.checked(&paths.root().join("launcher").join(name))?;
        }
        let mut db = self.database.connect()?;
        journal::recover(&mut db, &paths)?;
        action(&mut db, &paths)
    }

    pub fn snapshot(&self) -> Result<LibrarySnapshot, CoreError> {
        self.access(|db, paths| {
            let mut result = repository::snapshot(db)?;
            for instance in &mut result.instances {
                let directory = paths.location("instances", &instance.id)?;
                if paths
                    .checked(&directory.join(".sporium/project-change.json"))?
                    .exists()
                {
                    continue;
                }
                let Ok(project) = crate::projects::manifest(paths, &directory) else {
                    continue;
                };
                let kind = project.map_or_else(
                    || instance.loader.into(),
                    |p| {
                        if p.creator_studio {
                            InstanceType::CreatorStudio
                        } else {
                            InstanceType::ManagedProject
                        }
                    },
                );
                if instance.instance_type != kind {
                    instance.instance_type = kind;
                    instance.revision = next_revision(instance.revision)?;
                    instance.updated_at = now();
                    repository::update_instance(db, instance)?;
                }
            }
            Ok(result)
        })
    }

    pub fn create(&self, request: CreateInstance) -> Result<LibraryChange, CoreError> {
        self.create_with_icon(request, None)
    }
    // Import publishes only a fully verified private tree through the existing recovery journal.
    pub(crate) fn import_prepared(
        &self,
        request: CreateInstance,
        loader_version: Option<String>,
        source: &Path,
        provenance: String,
    ) -> Result<LibraryChange, CoreError> {
        let mut instance = Self::draft(&request)?;
        if let Some(version) = &loader_version {
            crate::game::loaders::valid_version(version)?;
            if instance.loader == Loader::Vanilla {
                return Err(CoreError::InvalidInput);
            }
        }
        instance.loader_version = loader_version;
        instance.provider_refs = vec![provenance];
        self.access(|db, paths| {
            let source = paths.checked(source)?;
            paths.check_tree(&source, 0)?;
            Self::publish(db, paths, instance, Some(source))
        })
    }
    pub(crate) fn draft(request: &CreateInstance) -> Result<Instance, CoreError> {
        let name = display_name(&request.name)?;
        let minecraft_version = game_version(&request.minecraft_version)?;
        let time = now();
        let mut instance = Instance {
            schema_version: INSTANCE_SCHEMA,
            id: uuid::Uuid::new_v4().to_string(),
            revision: 0,
            name,
            minecraft_version,
            loader: request.loader,
            loader_version: None,
            instance_type: request.loader.into(),
            collection_id: request.collection_id.clone(),
            status: InstallStatus::NotInstalled,
            java_mode: AutoMode::Auto,
            memory_mode: AutoMode::Auto,
            last_account_id: None,
            default_account_id: None,
            created_at: time,
            updated_at: time,
            last_played_at: None,
            favorite: false,
            icon_source: IconSource::Automatic,
            managed_shortcut: None,
            icon_ref: None,
            cover_ref: None,
            provider_refs: vec![],
            installed_content_ids: vec![],
        };
        instance.icon_ref = icons::automatic_icon(&instance.id)?;

        Ok(instance)
    }
    pub(crate) fn create_with_icon(
        &self,
        request: CreateInstance,
        icon: Option<String>,
    ) -> Result<LibraryChange, CoreError> {
        let mut instance = Self::draft(&request)?;
        if let Some(icon) = icon {
            instance.icon_ref = Some(icon);
        }
        self.access(|db, paths| {
            repository::require_collection(db, &request.collection_id)?;
            Self::publish(db, paths, instance, None)
        })
    }

    fn publish(
        db: &mut Connection,
        paths: &Paths,
        instance: Instance,
        source: Option<PathBuf>,
    ) -> Result<LibraryChange, CoreError> {
        let mut pending = Pending {
            schema_version: INSTANCE_SCHEMA,
            phase: Phase::Building,
            instance,
            mode: None,
        };
        journal::save(db, &pending)?;
        let prepare = || -> Result<(), CoreError> {
            let stage = paths.prepare(&pending.instance.id)?;
            if let Some(source) = source {
                paths.copy_tree(&source, &stage, 0)?;
            }
            for directory in INSTANCE_DIRECTORIES {
                paths.mkdir(&stage.join(directory))?;
            }
            Ok(())
        };
        if let Err(error) = prepare() {
            // Keep the journal if cleanup cannot finish; startup recovery retries it.
            let _ = journal::finish(db, paths, &pending);
            return Err(error);
        }
        pending.phase = Phase::Publish;
        journal::save(db, &pending)?;
        journal::finish(db, paths, &pending)?;
        tracing::info!("instance_published");
        repository::change(db, pending.instance.id, None)
    }

    pub fn update(&self, request: UpdateInstance) -> Result<LibraryChange, CoreError> {
        let name = display_name(&request.name)?;
        self.access(|db, paths| {
            let mut value = repository::instance(db, &request.id)?;
            repository::revision(value.revision, request.expected_revision)?;
            repository::require_collection(db, &request.collection_id)?;
            paths.verify_marker(&paths.location("instances", &value.id)?, &value.id)?;
            value.name = name;
            value.collection_id = request.collection_id;
            value.revision = next_revision(value.revision)?;
            value.updated_at = now();
            repository::update_instance(db, &value)?;
            repository::change(db, value.id, None)
        })
    }

    pub fn duplicate(&self, request: DuplicateInstance) -> Result<LibraryChange, CoreError> {
        let name = display_name(&request.name)?;
        self.access(|db, paths| {
            let _game_guard = paths.instance_lock(&request.id)?;
            let mut instance = repository::instance(db, &request.id)?;
            repository::revision(instance.revision, request.expected_revision)?;
            let source = paths.location("instances", &instance.id)?;
            paths.verify_marker(&source, &instance.id)?;
            paths.check_tree(&source, 0)?;
            instance.id = uuid::Uuid::new_v4().to_string();
            instance.name = name;
            instance.revision = 0;
            instance.created_at = now();
            instance.updated_at = instance.created_at;
            instance.last_played_at = None;
            instance.managed_shortcut = None;
            instance.status = InstallStatus::NotInstalled;
            Self::publish(db, paths, instance, Some(source))
        })
    }

    pub fn delete(&self, request: DeleteInstance) -> Result<LibraryChange, CoreError> {
        self.access(|db, paths| {
            let _game_guard = paths.instance_lock(&request.id)?;
            let instance = repository::instance(db, &request.id)?;
            repository::revision(instance.revision, request.expected_revision)?;
            let directory = paths.location("instances", &instance.id)?;
            paths.verify_marker(&directory, &instance.id)?;
            paths.check_tree(&directory, 0)?;
            let pending = Pending {
                schema_version: INSTANCE_SCHEMA,
                phase: Phase::Delete,
                instance,
                mode: Some(request.mode),
            };
            journal::save(db, &pending)?;
            journal::finish(db, paths, &pending)?;
            let preserved = matches!(request.mode, DeleteMode::PreserveWorlds)
                .then(|| {
                    paths
                        .location("backups", &request.id)
                        .map(|p| p.to_string_lossy().into_owned())
                })
                .transpose()?;
            tracing::info!("instance_deleted");
            repository::change(db, request.id, preserved)
        })
    }

    pub fn folder(&self, request: OpenFolder) -> Result<PathBuf, CoreError> {
        self.access(|db, paths| {
            let path = match request.target {
                FolderTarget::Data => {
                    if request.id.is_some() {
                        return Err(CoreError::InvalidInput);
                    }
                    paths.root().to_path_buf()
                }
                FolderTarget::Instance | FolderTarget::Mods | FolderTarget::Logs => {
                    let id = request.id.as_deref().ok_or(CoreError::InvalidInput)?;
                    repository::instance(db, id)?;
                    let root = paths.location("instances", id)?;
                    paths.verify_marker(&root, id)?;
                    if matches!(request.target, FolderTarget::Mods) {
                        paths.checked(&root.join("mods"))?
                    } else if matches!(request.target, FolderTarget::Logs) {
                        paths.checked(&root.join("logs"))?
                    } else {
                        root
                    }
                }
                FolderTarget::Backup => {
                    let id = request.id.as_deref().ok_or(CoreError::InvalidInput)?;
                    let root = paths.location("backups", id)?;
                    paths.verify_marker(&root, id)?;
                    root
                }
            };
            paths.checked(&path)?;
            if !path.is_dir() {
                return Err(CoreError::NotFound);
            }
            Ok(path)
        })
    }

    pub fn open_folder(&self, request: OpenFolder) -> Result<(), CoreError> {
        opener::open(self.folder(request)?).map_err(|_| CoreError::OpenFolderFailed)
    }

    pub fn set_favorite(&self, request: SetFavorite) -> Result<LibraryChange, CoreError> {
        self.access(|db, _| {
            let mut instance = repository::instance(db, &request.id)?;
            repository::revision(instance.revision, request.expected_revision)?;
            instance.favorite = request.favorite;
            instance.revision = next_revision(instance.revision)?;
            instance.updated_at = now();
            repository::update_instance(db, &instance)?;
            repository::change(db, instance.id, None)
        })
    }

    pub(crate) fn lease_game(
        &self,
        id: &str,
    ) -> Result<(Instance, PathBuf, std::fs::File), CoreError> {
        self.access(|db, paths| {
            let instance = repository::instance(db, id)?;
            let directory = paths.location("instances", id)?;
            paths.verify_marker(&directory, id)?;
            let guard = paths.instance_lock(id)?;
            crate::content::install::recover(paths, &directory)?;
            Ok((instance, directory, guard))
        })
    }

    pub(crate) fn mark_game(&self, id: &str, launched: bool) -> Result<(), CoreError> {
        self.access(|db, _| {
            let mut instance = repository::instance(db, id)?;
            instance.status = InstallStatus::Installed;
            if launched {
                instance.last_played_at = Some(now());
            }
            instance.updated_at = now();
            instance.revision = next_revision(instance.revision)?;
            repository::update_instance(db, &instance)
        })
    }
    pub(crate) fn lease_content_read(
        &self,
        id: &str,
    ) -> Result<(PathBuf, std::fs::File), CoreError> {
        self.access(|db, paths| {
            repository::instance(db, id)?;
            let directory = paths.location("instances", id)?;
            paths.verify_marker(&directory, id)?;
            if paths
                .checked(&directory.join(".sporium/project-change.json"))?
                .exists()
                || paths
                    .checked(&directory.join(".sporium/content-pending.json"))?
                    .exists()
                || paths
                    .checked(&directory.join(".sporium/content-change.json"))?
                    .exists()
                || paths
                    .checked(&directory.join(".sporium/content-update.json"))?
                    .exists()
                || paths
                    .checked(&directory.join(".sporium/content-restore.json"))?
                    .exists()
                || paths
                    .checked(&directory.join(".sporium/content-adoption.json"))?
                    .exists()
            {
                let _exclusive = paths.instance_lock(id)?;
                crate::content::install::recover(paths, &directory)?;
            }
            let guard = paths.instance_read_lock(id)?;
            Ok((directory, guard))
        })
    }

    pub fn configure_launch(&self, request: ConfigureLaunch) -> Result<LibraryChange, CoreError> {
        if let Some(version) = &request.loader_version {
            super::game::loaders::valid_version(version)?;
        }
        self.access(|db, paths| {
            let _lease = paths.instance_lock(&request.id)?;
            let mut instance = repository::instance(db, &request.id)?;
            repository::revision(instance.revision, request.expected_revision)?;
            if instance.loader == Loader::Vanilla && request.loader_version.is_some() {
                return Err(CoreError::InvalidInput);
            }
            if instance.loader_version != request.loader_version {
                instance.status = InstallStatus::NotInstalled;
            }
            instance.loader_version = request.loader_version;

            instance.revision = next_revision(instance.revision)?;
            repository::update_instance(db, &instance)?;
            repository::change(db, instance.id, None)
        })
    }
    pub(crate) fn pin_loader(
        &self,
        id: &str,
        loader_version: Option<&str>,
    ) -> Result<(), CoreError> {
        self.access(|db, _| {
            let mut instance = repository::instance(db, id)?;
            if let Some(version) = loader_version
                && instance.loader_version.as_deref() != Some(version)
            {
                instance.loader_version = Some(version.into());
                instance.revision = next_revision(instance.revision)?;
                instance.updated_at = now();
                repository::update_instance(db, &instance)?;
            }
            Ok(())
        })
    }

    /// Stable UUID contract; OS shortcut creation and launch routing belong to phase 16.
    pub fn shortcut_plan(&self, request: RecordRequest) -> Result<ShortcutPlan, CoreError> {
        self.access(|db, _| {
            let instance = repository::instance(db, &request.id)?;
            repository::revision(instance.revision, request.expected_revision)?;
            Ok(ShortcutPlan {
                instance_id: instance.id.clone(),
                arguments: vec!["--launch-instance".into(), instance.id],
                available: false,
            })
        })
    }
}
