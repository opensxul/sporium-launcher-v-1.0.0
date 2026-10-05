pub mod model;
#[cfg(test)]
mod tests;
pub(crate) mod transaction;

use crate::{
    content::{install, model::ContentRecord, modrinth::Modrinth, provider::ContentProvider},
    error::CoreError,
    game::{
        fs::{read_limited, write_atomic},
        network::{Download, Hash, Network, verify},
    },
    instances::{Library, filesystem::Paths, model::Instance},
    packs::PackManager,
};
use model::*;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

struct Prepared {
    created: Instant,
    view: ProjectPlan,
    manifest: ProjectManifest,
    desired: BTreeMap<String, Option<PathBuf>>,
    context: BTreeMap<String, Option<transaction::Blob>>,
    _tree: tempfile::TempDir,
    _pack: Option<crate::packs::PreparedProjectPack>,
    source: Option<(PathBuf, String)>,
}
#[derive(Clone)]
pub struct ProjectManager {
    provider: Arc<dyn ContentProvider>,
    library: Library,
    packs: PackManager,
    plans: Arc<Mutex<HashMap<String, Prepared>>>,
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}
struct Busy(Arc<AtomicBool>);
impl Drop for Busy {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn label(value: &str) -> Result<(), CoreError> {
    if value.trim().is_empty() || value.len() > 160 || value.chars().any(char::is_control) {
        return Err(CoreError::InvalidInput);
    }
    Ok(())
}
pub(crate) fn validate(manifest: &ProjectManifest) -> Result<(), CoreError> {
    if manifest.schema_version != 1 {
        return Err(CoreError::SchemaTooNew);
    }
    label(&manifest.id)?;
    label(&manifest.version)?;
    label(&manifest.name)?;
    crate::instances::model::game_version(&manifest.minecraft)?;
    if let Some(version) = &manifest.loader_version {
        crate::game::loaders::valid_version(version)?;
    }
    if (manifest.loader == crate::instances::model::Loader::Vanilla)
        != manifest.loader_version.is_none()
    {
        return Err(CoreError::ContentIncompatible);
    }
    if manifest
        .launch
        .memory_mib
        .is_some_and(|m| !(256..=32768).contains(&m))
        || manifest.files.len() > 10000
        || manifest.optional_groups.len() > 256
    {
        return Err(CoreError::InvalidInput);
    }
    let mut groups = HashSet::new();
    for group in &manifest.optional_groups {
        label(group)?;
        if !groups.insert(group) {
            return Err(CoreError::InvalidInput);
        }
    }
    let mut names = HashSet::new();
    let mut bytes = 0u64;
    for file in &manifest.files {
        transaction::safe_path(&file.path)?;
        if file.path.starts_with("saves/") {
            return Err(CoreError::UnsafePath);
        }
        if file.path.starts_with(".sporium/")
            || !names.insert(file.path.to_lowercase())
            || !crate::packs::archive::hash_valid(&file.sha256, 64)
            || !crate::packs::archive::hash_valid(&file.sha512, 128)
            || file.size > 500_000_000
        {
            return Err(CoreError::Integrity);
        }
        if file
            .source
            .as_ref()
            .is_some_and(|s| !crate::packs::archive::pack_url(s))
        {
            return Err(CoreError::UnsafePath);
        }
        if let Some(group) = &file.group
            && (file.policy != FilePolicy::Optional || !groups.contains(group))
        {
            return Err(CoreError::InvalidInput);
        }
        if file.policy == FilePolicy::Optional && file.group.is_none() {
            return Err(CoreError::InvalidInput);
        }
        bytes = bytes
            .checked_add(file.size)
            .ok_or(CoreError::InvalidInput)?;
    }
    if bytes > 2_000_000_000 {
        return Err(CoreError::InvalidInput);
    }
    crate::packs::archive::reject_file_parents(manifest.files.iter().map(|f| f.path.as_str()))?;
    if let Some(source) = &manifest.pack_source {
        crate::content::modrinth::valid_id(&source.project_id)?;
        crate::content::modrinth::valid_id(&source.version_id)?;
    }
    Ok(())
}
pub(crate) fn manifest(
    paths: &Paths,
    directory: &Path,
) -> Result<Option<ProjectManifest>, CoreError> {
    let file = paths.checked(&directory.join(".sporium/project.json"))?;
    if !file.exists() {
        return Ok(None);
    }
    let value: ProjectManifest = serde_json::from_slice(&read_limited(paths, &file, 8_000_000)?)?;
    validate(&value)?;
    Ok(Some(value))
}
fn compatible(manifest: &ProjectManifest, instance: &Instance) -> Result<(), CoreError> {
    validate(manifest)?;
    if manifest.minecraft != instance.minecraft_version
        || manifest.loader != instance.loader
        || manifest.loader_version != instance.loader_version
    {
        return Err(CoreError::ContentIncompatible);
    }
    Ok(())
}
fn pack_source(paths: &Paths, directory: &Path) -> Result<Option<PackSource>, CoreError> {
    if let Some(value) = manifest(paths, directory)? {
        return Ok(value.pack_source);
    }
    let file = paths.checked(&directory.join(".sporium/import.json"))?;
    if !file.exists() {
        return Ok(None);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&read_limited(paths, &file, 16_000_000)?)?;
    let Some(items) = value.get("providerRefs").and_then(|v| v.as_array()) else {
        return Ok(None);
    };
    for item in items {
        if let Some(item) = item
            .as_str()
            .and_then(|s| s.strip_prefix("modrinth_pack_ref:"))
        {
            let parts: Vec<_> = item.split(':').collect();
            if parts.len() != 3 || !crate::packs::archive::hash_valid(parts[2], 128) {
                return Err(CoreError::Integrity);
            }
            crate::content::modrinth::valid_id(parts[0])?;
            crate::content::modrinth::valid_id(parts[1])?;
            return Ok(Some(PackSource {
                project_id: parts[0].into(),
                version_id: parts[1].into(),
            }));
        }
    }
    Ok(None)
}
fn blob_at(paths: &Paths, path: &Path) -> Result<Option<transaction::Blob>, CoreError> {
    let path = paths.checked(path)?;
    if path.exists() {
        Ok(Some(transaction::digest(&path)?))
    } else {
        Ok(None)
    }
}
fn collect(paths: &Paths, directory: &Path) -> Result<Vec<String>, CoreError> {
    let mut pending = vec![];
    for name in [
        "mods",
        "mods_disabled",
        "resourcepacks",
        "shaderpacks",
        "config",
        "defaultconfigs",
        "kubejs",
        "scripts",
        "datapacks",
        "options.txt",
        "optionsof.txt",
        "optionsshaders.txt",
        "servers.dat",
    ] {
        pending.push(directory.join(name));
    }
    let mut result = vec![];
    let mut bytes = 0u64;
    while let Some(path) = pending.pop() {
        let path = paths.checked(&path)?;
        if !path.exists() {
            continue;
        }
        if path.is_dir() {
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
        } else {
            bytes += path.metadata()?.len();
            if bytes > 2_000_000_000 {
                return Err(CoreError::InvalidInput);
            }
            result.push(
                path.strip_prefix(directory)
                    .map_err(|_| CoreError::UnsafePath)?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
        if pending.len() + result.len() > 10000 {
            return Err(CoreError::InvalidInput);
        }
    }
    result.sort();
    Ok(result)
}

impl ProjectManager {
    pub fn new(library: Library, packs: PackManager) -> Result<Self, CoreError> {
        Ok(Self {
            provider: Arc::new(Modrinth::new(library.root().to_path_buf())?),
            library,
            packs,
            plans: Arc::new(Mutex::new(HashMap::new())),
            busy: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(AtomicBool::new(false)),
        })
    }
    fn work(&self) -> Result<Busy, CoreError> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err(CoreError::InstanceBusy);
        }
        self.cancel.store(false, Ordering::SeqCst);
        Ok(Busy(self.busy.clone()))
    }
    pub fn is_active(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.packs.cancel();
    }
    fn instance(&self, id: &str) -> Result<Instance, CoreError> {
        self.library
            .snapshot()?
            .instances
            .into_iter()
            .find(|i| i.id == id)
            .ok_or(CoreError::NotFound)
    }
    pub fn view(&self, id: &str) -> Result<ProjectView, CoreError> {
        let (directory, _lease) = self.library.lease_content_read(id)?;
        let paths = Paths::new(self.library.root())?;
        let file = paths.checked(&directory.join(".sporium/project-source.json"))?;
        let local_source = if file.exists() {
            let value: serde_json::Value =
                serde_json::from_slice(&read_limited(&paths, &file, 8192)?)?;
            value
                .get("path")
                .and_then(|s| s.as_str())
                .and_then(|s| Path::new(s).file_name())
                .map(|s| s.to_string_lossy().into_owned())
        } else {
            None
        };
        Ok(ProjectView {
            manifest: manifest(&paths, &directory)?,
            pack_source: pack_source(&paths, &directory)?,
            local_source,
        })
    }
    pub fn studio_files(&self, id: &str) -> Result<Vec<ProjectFile>, CoreError> {
        let (directory, _lease) = self.library.lease_content_read(id)?;
        let paths = Paths::new(self.library.root())?;
        let records = install::records(&paths, &directory)?;
        let existing = manifest(&paths, &directory)?;
        collect(&paths, &directory)?
            .into_iter()
            .map(|name| {
                let blob = transaction::digest(&directory.join(&name))?;
                let prior = existing
                    .as_ref()
                    .and_then(|m| m.files.iter().find(|f| f.path == name));
                let source = records
                    .iter()
                    .find(|r| {
                        format!("{}/{}", r.directory, r.file.filename) == name
                            && r.file.hashes.sha512 == blob.sha512
                            && r.provider == "modrinth"
                    })
                    .map(|r| r.file.url.clone());
                Ok(ProjectFile {
                    path: name.clone(),
                    sha256: blob.sha256,
                    sha512: blob.sha512,
                    size: blob.size,
                    source,
                    policy: prior.map_or(
                        if transaction::is_setting(&name) {
                            FilePolicy::UserAllowed
                        } else {
                            FilePolicy::RequiredLocked
                        },
                        |f| f.policy,
                    ),
                    group: prior.and_then(|f| f.group.clone()),
                })
            })
            .collect()
    }
    fn remember(&self, value: Prepared) -> ProjectPlan {
        let view = value.view.clone();
        let mut plans = self.plans.lock().unwrap();
        plans.retain(|_, p| p.created.elapsed() < Duration::from_secs(900));
        if plans.len() >= 8
            && let Some(id) = plans.keys().next().cloned()
        {
            plans.remove(&id);
        }
        plans.insert(view.token.clone(), value);
        view
    }
    pub fn studio_plan(&self, request: StudioRequest) -> Result<ProjectPlan, CoreError> {
        let _busy = self.work()?;
        label(&request.name)?;
        label(&request.version)?;
        let instance = self.instance(&request.id)?;
        let paths = Paths::new(self.library.root())?;
        let (directory, _lease) = self.library.lease_content_read(&request.id)?;
        let old = manifest(&paths, &directory)?;
        let mut files = self.studio_files(&request.id)?;
        if request.policies.len() != files.len() {
            return Err(CoreError::RecordConflict);
        }
        let mut seen = HashSet::new();
        let mut groups = HashSet::new();
        for item in request.policies {
            if !seen.insert(item.path.clone()) {
                return Err(CoreError::InvalidInput);
            }
            let file = files
                .iter_mut()
                .find(|f| f.path == item.path)
                .ok_or(CoreError::RecordConflict)?;
            file.policy = item.policy;
            file.group = item.group;
            if let Some(group) = &file.group {
                groups.insert(group.clone());
            }
        }
        let value = ProjectManifest {
            schema_version: 1,
            id: old
                .as_ref()
                .map_or_else(|| uuid::Uuid::new_v4().to_string(), |m| m.id.clone()),
            version: request.version,
            name: request.name,
            minecraft: instance.minecraft_version.clone(),
            loader: instance.loader,
            loader_version: instance.loader_version.clone(),
            creator_studio: true,
            forbid_external_mods: request.forbid_external_mods,
            files,
            optional_groups: groups.into_iter().collect(),
            launch: ProjectLaunch::default(),
            pack_source: None,
        };
        validate(&value)?;
        for file in &value.files {
            let cache = paths.checked(
                &paths
                    .root()
                    .join("launcher/project-sources")
                    .join(format!("{}.bin", file.sha512)),
            )?;
            if !cache.exists() {
                paths.mkdir(cache.parent().ok_or(CoreError::UnsafePath)?)?;
                write_copy(&paths, &directory.join(&file.path), &cache, &file.blob())?;
            }
        }
        drop(_lease);
        let groups = value.optional_groups.clone();
        self.prepare(&request.id, value, &groups, "studio", None, None)
    }
    pub fn local_plan(
        &self,
        id: &str,
        file: &Path,
        groups: &[String],
    ) -> Result<ProjectPlan, CoreError> {
        let _busy = self.work()?;
        crate::instances::filesystem::no_links(file)?;
        if !file.is_absolute() || file.metadata()?.len() > 8_000_000 {
            return Err(CoreError::InvalidInput);
        }
        let bytes = fs::read(file)?;
        let hash = crate::game::network::hex(&sha2::Sha512::digest(&bytes));
        let value: ProjectManifest = serde_json::from_slice(&bytes)?;
        self.prepare(
            id,
            value,
            groups,
            "project_update",
            None,
            Some((file.to_path_buf(), hash)),
        )
    }
    pub fn repair_plan(&self, id: &str) -> Result<ProjectPlan, CoreError> {
        let view = self.view(id)?;
        if view.manifest.is_none() {
            let source = view.pack_source.ok_or(CoreError::NotFound)?;
            let mut plan = self.pack_plan(id, &source.version_id)?;
            plan.action = "project_repair".into();
            if let Some(prepared) = self.plans.lock().unwrap().get_mut(&plan.token) {
                prepared.view.action = plan.action.clone();
            }
            return Ok(plan);
        }
        let _busy = self.work()?;
        let value = view.manifest.ok_or(CoreError::NotFound)?;
        let directory = self.library.root().join("instances").join(id);
        let groups: Vec<_> = value
            .files
            .iter()
            .filter(|f| f.policy == FilePolicy::Optional && directory.join(&f.path).exists())
            .filter_map(|f| f.group.clone())
            .collect();
        self.prepare(id, value, &groups, "project_repair", None, None)
    }
    pub fn pack_plan(&self, id: &str, version_id: &str) -> Result<ProjectPlan, CoreError> {
        let _busy = self.work()?;
        self.pack_plan_groups(id, version_id, None)
    }
    fn pack_plan_groups(
        &self,
        id: &str,
        version_id: &str,
        selected: Option<&[String]>,
    ) -> Result<ProjectPlan, CoreError> {
        let view = self.view(id)?;
        let source = view.pack_source.ok_or(CoreError::NotFound)?;
        let instance = self.instance(id)?;
        let preview = self
            .packs
            .provider_preview(&source.project_id, version_id)?;
        let directory = self.library.root().join("instances").join(id);
        let selected_groups: Vec<String> = selected.map_or_else(
            || {
                preview
                    .optional_files
                    .iter()
                    .filter(|name| directory.join(name).is_file())
                    .cloned()
                    .collect()
            },
            |groups| groups.to_vec(),
        );
        if selected_groups
            .iter()
            .any(|group| !preview.optional_files.contains(group))
        {
            return Err(CoreError::InvalidInput);
        }
        let pack =
            self.packs
                .prepare_existing(&preview.token, &instance, &preview.optional_files)?;
        let paths = Paths::new(self.library.root())?;
        let mut files = vec![];
        for name in collect(&paths, &pack.game)? {
            let blob = transaction::digest(&pack.game.join(&name))?;
            let source = pack
                .manifest
                .files
                .iter()
                .find(|f| f.path == name)
                .and_then(|f| f.downloads.first())
                .cloned();
            files.push(ProjectFile {
                path: name.clone(),
                sha256: blob.sha256,
                sha512: blob.sha512,
                size: blob.size,
                source,
                policy: if preview.optional_files.contains(&name) {
                    FilePolicy::Optional
                } else if transaction::is_setting(&name) {
                    FilePolicy::UserAllowed
                } else {
                    FilePolicy::RequiredLocked
                },
                group: preview
                    .optional_files
                    .contains(&name)
                    .then_some(name.clone()),
            });
        }
        let value = ProjectManifest {
            schema_version: 1,
            id: source.project_id.clone(),
            version: pack.manifest.version_id.clone(),
            name: pack.manifest.name.clone(),
            minecraft: instance.minecraft_version.clone(),
            loader: instance.loader,
            loader_version: instance.loader_version.clone(),
            creator_studio: false,
            forbid_external_mods: false,
            files,
            optional_groups: preview.optional_files,
            launch: ProjectLaunch::default(),
            pack_source: Some(PackSource {
                project_id: source.project_id,
                version_id: version_id.into(),
            }),
        };
        self.prepare(
            id,
            value,
            &selected_groups,
            "project_update",
            Some(pack),
            None,
        )
    }
    fn prepare(
        &self,
        id: &str,
        value: ProjectManifest,
        groups: &[String],
        action: &str,
        pack: Option<crate::packs::PreparedProjectPack>,
        source: Option<(PathBuf, String)>,
    ) -> Result<ProjectPlan, CoreError> {
        let paths = Paths::new(self.library.root())?;
        let instance = self.instance(id)?;
        compatible(&value, &instance)?;
        if groups.iter().any(|g| !value.optional_groups.contains(g)) {
            return Err(CoreError::InvalidInput);
        }
        let (directory, lease) = self.library.lease_content_read(id)?;
        let old = manifest(&paths, &directory)?;
        if old.as_ref().is_some_and(|m| m.id != value.id) && action != "studio" {
            return Err(CoreError::ContentConflict);
        }
        let old_paths: HashSet<_> = old.as_ref().map_or(HashSet::new(), |m| {
            m.files
                .iter()
                .filter(|f| f.policy != FilePolicy::UserAllowed)
                .map(|f| f.path.clone())
                .collect()
        });
        let mut owned = old_paths.clone();
        let mut expected: BTreeMap<String, String> = old.as_ref().map_or(BTreeMap::new(), |m| {
            m.files
                .iter()
                .map(|f| (f.path.clone(), f.sha512.clone()))
                .collect()
        });
        let mut names: HashSet<String> = value
            .files
            .iter()
            .map(|f| f.path.clone())
            .chain(old_paths.iter().cloned())
            .collect();
        // Imported packs retain the originally declared ownership, never adopt subsequently added files.
        if old.is_none() && pack.is_some() {
            let import = directory.join(".sporium/import.json");
            if import.exists() {
                let metadata: serde_json::Value =
                    serde_json::from_slice(&read_limited(&paths, &import, 16_000_000)?)?;
                for file in metadata
                    .get("selectedFiles")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                {
                    if let Some(name) = file.get("path").and_then(|p| p.as_str())
                        && transaction::safe_path(name).is_ok()
                    {
                        names.insert(name.into());
                        owned.insert(name.into());
                        if let Some(hash) = file
                            .get("hashes")
                            .and_then(|h| h.get("sha512"))
                            .and_then(|h| h.as_str())
                        {
                            expected.insert(name.into(), hash.into());
                        }
                    }
                }
                if let Some(files) = metadata.get("embedded").and_then(|v| v.as_object()) {
                    for name in files.keys() {
                        if transaction::safe_path(name).is_ok() && !transaction::is_setting(name) {
                            names.insert(name.clone());
                            owned.insert(name.clone());
                            if let Some(hash) = files[name].get("sha512").and_then(|h| h.as_str()) {
                                expected.insert(name.clone(), hash.into());
                            }
                        }
                    }
                }
            }
        }
        names.extend([
            ".sporium/project.json".into(),
            ".sporium/content.json".into(),
            ".sporium/import.json".into(),
            ".sporium/project-source.json".into(),
            ".sporium/content-update-policy.json".into(),
        ]);
        let jars: Vec<String> = collect(&paths, &directory)?
            .into_iter()
            .filter(|n| {
                (n.starts_with("mods/") || n.starts_with("mods_disabled/"))
                    && n.to_ascii_lowercase().ends_with(".jar")
            })
            .collect();
        names.extend(jars.iter().cloned());
        let mut context = BTreeMap::new();
        for name in &names {
            transaction::safe_path(name)?;
            context.insert(name.clone(), blob_at(&paths, &directory.join(name))?);
        }
        let current_records = install::records(&paths, &directory)?;
        let pins = crate::content::project_update_policies(&paths, &directory)?;
        drop(lease);
        let stage_root = paths.checked(&paths.root().join("launcher/project-plans"))?;
        paths.mkdir(&stage_root)?;
        let tree = tempfile::Builder::new()
            .prefix("plan-")
            .tempdir_in(stage_root)?;
        let mut desired = BTreeMap::new();
        let mut warnings = vec![];
        for file in &value.files {
            if self.cancel.load(Ordering::SeqCst) {
                return Err(CoreError::Cancelled);
            }
            let present = context[&file.path].as_ref();
            if file.policy == FilePolicy::UserForbidden {
                if present.is_some() {
                    warnings.push(format!("forbidden: {}", file.path));
                    desired.insert(file.path.clone(), None);
                }
                continue;
            }
            if file.policy == FilePolicy::UserAllowed && present.is_some() {
                continue;
            }
            if file.policy == FilePolicy::Optional
                && !file.group.as_ref().is_some_and(|g| groups.contains(g))
            {
                if present.is_some() && (owned.contains(&file.path) || action == "studio") {
                    warnings.push(format!("remove: {}", file.path));
                    if present.is_some_and(|b| expected.get(&file.path) != Some(&b.sha512)) {
                        warnings.push(format!("modified: {}", file.path));
                    }
                    desired.insert(file.path.clone(), None);
                }
                continue;
            }
            if present.is_some_and(|b| b == &file.blob()) {
                continue;
            }
            if present.is_some()
                && action != "studio"
                && !owned.contains(&file.path)
                && file.policy != FilePolicy::UserAllowed
            {
                return Err(CoreError::ContentConflict);
            }
            if present.is_some() {
                warnings.push(format!("replace: {}", file.path));
                if present.is_some_and(|b| expected.get(&file.path) != Some(&b.sha512)) {
                    warnings.push(format!("modified: {}", file.path));
                }
            }
            let staged = paths.checked(&tree.path().join(&file.path))?;
            paths.mkdir(staged.parent().ok_or(CoreError::UnsafePath)?)?;
            let pack_file = pack.as_ref().map(|p| p.game.join(&file.path));
            if let Some(pack_file) = pack_file.filter(|p| p.exists()) {
                write_copy(&paths, &pack_file, &staged, &file.blob())?;
            } else {
                let cached = paths.checked(
                    &paths
                        .root()
                        .join("launcher/project-sources")
                        .join(format!("{}.bin", file.sha512)),
                )?;
                if !cached.exists()
                    || !verify(&cached, &Hash::Sha256(file.sha256.clone()), file.size)?
                {
                    let url = file.source.clone().ok_or(CoreError::ContentUnsupported)?;
                    let _download =
                        paths.named_lock(&paths.root().join("shared/cache/downloads.lock"))?;
                    Network::new()?.download(
                        &paths,
                        &Download {
                            url,
                            path: cached.clone(),
                            hash: Hash::Sha512(file.sha512.clone()),
                            size: file.size,
                        },
                        &|| self.cancel.load(Ordering::SeqCst),
                        &|_| {},
                    )?;
                }
                write_copy(&paths, &cached, &staged, &file.blob())?;
            }
            desired.insert(file.path.clone(), Some(staged));
        }
        for name in owned
            .iter()
            .filter(|n| !n.starts_with(".sporium/") && !transaction::is_setting(n))
        {
            if !value.files.iter().any(|f| &f.path == name) && context[name].is_some() {
                warnings.push(format!("remove: {name}"));
                if context[name]
                    .as_ref()
                    .is_some_and(|b| expected.get(name) != Some(&b.sha512))
                {
                    warnings.push(format!("modified: {name}"));
                }
                desired.insert(name.clone(), None);
            }
        }
        let manifest_file = tree.path().join("project.json");
        for record in &current_records {
            let name = format!("{}/{}", record.directory, record.file.filename);
            if let Some(next) = desired.get(&name) {
                let changed = match next {
                    Some(file) => transaction::digest(file)?.sha512 != record.file.hashes.sha512,
                    None => true,
                };
                if changed
                    && pins.iter().any(|p| {
                        p.project_id == record.project_id
                            && (p.pinned || p.ignored_version.is_some())
                    })
                {
                    return Err(CoreError::ContentConflict);
                }
            }
        }
        write_atomic(&paths, &manifest_file, &serde_json::to_vec(&value)?)?;
        desired.insert(".sporium/project.json".into(), Some(manifest_file));
        let mut records: Vec<ContentRecord> = current_records
            .into_iter()
            .filter(|r| {
                let name = format!("{}/{}", r.directory, r.file.filename);
                match desired.get(&name) {
                    None => true,
                    Some(Some(file)) => {
                        transaction::digest(file).is_ok_and(|b| b.sha512 == r.file.hashes.sha512)
                    }
                    Some(None) => false,
                }
            })
            .collect();
        if let Some(pack) = &pack {
            for record in &pack.records {
                let name = format!("{}/{}", record.directory, record.file.filename);
                if value.files.iter().any(|file| {
                    file.path == name
                        && file.policy == FilePolicy::Optional
                        && !file
                            .group
                            .as_ref()
                            .is_some_and(|group| groups.contains(group))
                }) {
                    continue;
                }
                if !records.iter().any(|r| {
                    r.directory == record.directory && r.file.filename == record.file.filename
                }) {
                    records.push(record.clone());
                }
            }
            if pack.game.join(".sporium/import.json").exists() {
                desired.insert(
                    ".sporium/import.json".into(),
                    Some(pack.game.join(".sporium/import.json")),
                );
            }
        } else {
            let proven = crate::packs::receipts(
                &paths,
                tree.path(),
                &instance,
                self.provider.as_ref(),
                &self.cancel,
            )?;
            for record in proven {
                if !records.iter().any(|r| {
                    r.directory == record.directory && r.file.filename == record.file.filename
                }) {
                    records.push(record);
                }
            }
        }
        for name in &jars {
            match desired.get(name) {
                Some(None) => continue,
                Some(Some(_)) => continue,
                None => (),
            }
            let target = paths.checked(&tree.path().join(name))?;
            if !target.exists()
                && let Some(blob) = &context[name]
            {
                write_copy(&paths, &directory.join(name), &target, blob)?;
            }
        }
        let diagnostics = crate::content::project_diagnostics(&paths, tree.path(), &instance)?;
        if diagnostics.errors > 0 {
            return Err(CoreError::DependencyConflict);
        }
        if !diagnostics.complete || diagnostics.warnings > 0 {
            warnings.push("unverified: dependency metadata".into());
        }
        install::write_records(&paths, tree.path(), &records)?;
        desired.insert(
            ".sporium/content.json".into(),
            Some(tree.path().join(".sporium/content.json")),
        );
        if let Some((file, _)) = &source {
            let pointer = tree.path().join("source.json");
            write_atomic(
                &paths,
                &pointer,
                &serde_json::to_vec(&serde_json::json!({"path":file}))?,
            )?;
            desired.insert(".sporium/project-source.json".into(), Some(pointer));
        }
        if self.cancel.load(Ordering::SeqCst) {
            return Err(CoreError::Cancelled);
        }
        let view = ProjectPlan {
            token: uuid::Uuid::new_v4().to_string(),
            instance_id: id.into(),
            action: action.into(),
            current: old.as_ref().map_or_else(
                || {
                    let file = directory.join(".sporium/import.json");
                    read_limited(&paths, &file, 16_000_000)
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                        .and_then(|v| {
                            v.get("manifest")
                                .and_then(|m| m.get("versionId"))
                                .and_then(|v| v.as_str())
                                .map(str::to_owned)
                        })
                        .unwrap_or("local".into())
                },
                |m| m.version.clone(),
            ),
            available: value.version.clone(),
            changes: desired
                .keys()
                .filter(|n| !n.starts_with(".sporium/"))
                .cloned()
                .collect(),
            warnings,
            optional_groups: value.optional_groups.clone(),
            selected_groups: groups.to_vec(),
        };
        Ok(self.remember(Prepared {
            created: Instant::now(),
            view,
            manifest: value,
            desired,
            context,
            _tree: tree,
            _pack: pack,
            source,
        }))
    }
    pub fn dismiss(&self, token: &str) {
        self.plans.lock().unwrap().remove(token);
    }
    pub fn replan(&self, token: &str, groups: &[String]) -> Result<ProjectPlan, CoreError> {
        let _busy = self.work()?;
        let plans = self.plans.lock().unwrap();
        let old = plans.get(token).ok_or(CoreError::NotFound)?;
        if old.created.elapsed() > Duration::from_secs(900) {
            return Err(CoreError::ContentUnsupported);
        }
        let id = old.view.instance_id.clone();
        let manifest = old.manifest.clone();
        let action = old.view.action.clone();
        let source = old.source.clone();
        let pack = old
            ._pack
            .is_some()
            .then(|| old.manifest.pack_source.clone())
            .flatten();
        drop(plans);
        let result = if let Some(pack) = pack {
            self.pack_plan_groups(&id, &pack.version_id, Some(groups))?
        } else {
            self.prepare(&id, manifest, groups, &action, None, source)?
        };
        self.dismiss(token);
        Ok(result)
    }
    pub fn apply(&self, token: &str, accept_changes: bool) -> Result<String, CoreError> {
        let _busy = self.work()?;
        let mut plans = self.plans.lock().unwrap();
        let plan = plans.get(token).ok_or(CoreError::NotFound)?;
        if plan.created.elapsed() > Duration::from_secs(900)
            || (!plan.view.warnings.is_empty() && !accept_changes)
        {
            return Err(CoreError::RecordConflict);
        }
        let (instance, directory, _lease) = self.library.lease_game(&plan.view.instance_id)?;
        compatible(&plan.manifest, &instance)?;
        let paths = Paths::new(self.library.root())?;
        for (name, blob) in &plan.context {
            if &blob_at(&paths, &directory.join(name))? != blob {
                return Err(CoreError::RecordConflict);
            }
        }
        if let Some((source, hash)) = &plan.source
            && !verify(
                source,
                &Hash::Sha512(hash.clone()),
                source.metadata()?.len(),
            )?
        {
            return Err(CoreError::RecordConflict);
        }
        let plan = plans.remove(token).ok_or(CoreError::NotFound)?;
        drop(plans);
        let point = transaction::apply(
            &paths,
            &directory,
            &instance,
            plan.desired,
            &plan.view.action,
            vec![format!(
                "{}: {} → {}",
                plan.manifest.name, plan.view.current, plan.view.available
            )],
        )?;
        // Kind is derived from the committed manifest by the library snapshot, including crash recovery.
        Ok(point)
    }
    pub fn check(&self, id: &str) -> Result<ProjectUpdate, CoreError> {
        let view = self.view(id)?;
        let instance = self.instance(id)?;
        let mut result = ProjectUpdate {
            current: view.manifest.as_ref().map_or_else(
                || {
                    view.pack_source
                        .as_ref()
                        .map_or("local".into(), |s| s.version_id.clone())
                },
                |m| m.version.clone(),
            ),
            available: None,
            version_id: None,
            status: "local".into(),
        };
        if let Some(source) = view.pack_source {
            let versions = match self.provider.versions(&source.project_id, "") {
                Ok(v) => v,
                Err(_) => {
                    result.status = "unavailable".into();
                    return Ok(result);
                }
            };
            let current = match self.provider.version(&source.version_id) {
                Ok(v) => v,
                Err(_) => {
                    result.status = "unavailable".into();
                    return Ok(result);
                }
            };
            result.current = current.version_number.clone();
            let mut newer: Vec<_> = versions
                .iter()
                .filter(|v| {
                    v.project_id == source.project_id
                        && v.id != current.id
                        && v.status == "listed"
                        && v.date_published > current.date_published
                        && (v.version_type == "release" || v.version_type == current.version_type)
                })
                .collect();
            newer.sort_by(|a, b| b.date_published.cmp(&a.date_published));
            let compatible = newer.iter().find(|v| {
                v.game_versions.contains(&instance.minecraft_version)
                    && v.loaders
                        .iter()
                        .any(|l| l == crate::content::resolve::loader_name(instance.loader))
            });
            if let Some(version) = compatible {
                result.status = "available".into();
                result.available = Some(version.version_number.clone());
                result.version_id = Some(version.id.clone());
            } else {
                result.status = if newer.is_empty() {
                    "current"
                } else {
                    "incompatible"
                }
                .into();
            }
        } else if view.local_source.is_some() {
            let paths = Paths::new(self.library.root())?;
            let directory = paths.root().join("instances").join(id);
            let pointer: serde_json::Value = serde_json::from_slice(&read_limited(
                &paths,
                &directory.join(".sporium/project-source.json"),
                8192,
            )?)?;
            let file = Path::new(
                pointer
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or(CoreError::Integrity)?,
            );
            let candidate = (|| {
                crate::instances::filesystem::no_links(file)?;
                if file.metadata()?.len() > 8_000_000 {
                    return Err(CoreError::Integrity);
                }
                let candidate: ProjectManifest = serde_json::from_slice(&fs::read(file)?)?;
                validate(&candidate)?;
                Ok::<_, CoreError>(candidate)
            })();
            match candidate {
                Ok(candidate) => {
                    if candidate.id != view.manifest.as_ref().ok_or(CoreError::NotFound)?.id {
                        result.status = "incompatible".into();
                    } else if candidate.version == result.current {
                        result.status = "current".into();
                    } else {
                        result.available = Some(candidate.version.clone());
                        result.status = if compatible(&candidate, &instance).is_ok() {
                            "available"
                        } else {
                            "incompatible"
                        }
                        .into();
                    }
                }
                Err(_) => result.status = "unavailable".into(),
            }
        }
        Ok(result)
    }
    pub fn source_plan(&self, id: &str, groups: &[String]) -> Result<ProjectPlan, CoreError> {
        let paths = Paths::new(self.library.root())?;
        let pointer: serde_json::Value = serde_json::from_slice(&read_limited(
            &paths,
            &paths
                .root()
                .join("instances")
                .join(id)
                .join(".sporium/project-source.json"),
            8192,
        )?)?;
        self.local_plan(
            id,
            Path::new(
                pointer
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or(CoreError::NotFound)?,
            ),
            groups,
        )
    }
    pub fn export(&self, id: &str, file: &Path) -> Result<(), CoreError> {
        let view = self.view(id)?;
        let manifest = view.manifest.ok_or(CoreError::NotFound)?;
        crate::instances::filesystem::no_links(file)?;
        if !file.is_absolute() || file.exists() || file.starts_with(self.library.root()) {
            return Err(CoreError::UnsafePath);
        }
        use std::io::Write;
        let mut target = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(file)?;
        target.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
        target.sync_all()?;
        Ok(())
    }
}

use sha2::Digest;
pub(crate) fn guard_user_file(
    paths: &Paths,
    directory: &Path,
    name: &str,
    installing: bool,
) -> Result<(), CoreError> {
    let Some(value) = manifest(paths, directory)? else {
        return Ok(());
    };
    let canonical = name
        .strip_prefix("mods_disabled/")
        .map_or_else(|| name.to_string(), |n| format!("mods/{n}"));
    let file = value
        .files
        .iter()
        .find(|f| f.path.eq_ignore_ascii_case(name) || f.path.eq_ignore_ascii_case(&canonical));
    if file.is_some_and(|f| {
        f.policy == FilePolicy::RequiredLocked
            || (installing && f.policy == FilePolicy::UserForbidden)
    }) {
        return Err(CoreError::ContentConflict);
    }
    if installing
        && value.forbid_external_mods
        && (name.starts_with("mods/") || name.starts_with("mods_disabled/"))
        && file.is_none()
    {
        return Err(CoreError::ContentConflict);
    }
    Ok(())
}
pub(crate) fn guard_restore(
    paths: &Paths,
    directory: &Path,
    records: &[ContentRecord],
) -> Result<(), CoreError> {
    let Some(value) = manifest(paths, directory)? else {
        return Ok(());
    };
    for file in &value.files {
        if file.policy == FilePolicy::RequiredLocked
            && (file.path.starts_with("mods/")
                || file.path.starts_with("resourcepacks/")
                || file.path.starts_with("shaderpacks/"))
            && !records.iter().any(|r| {
                format!("{}/{}", r.directory, r.file.filename) == file.path
                    && r.file.hashes.sha512 == file.sha512
            })
        {
            return Err(CoreError::ContentConflict);
        }
    }
    Ok(())
}
pub(crate) fn restore_points(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
) -> Result<Vec<crate::content::model::ContentRestorePoint>, CoreError> {
    let folder = paths.checked(&directory.join(".sporium/project-snapshots"))?;
    if !folder.exists() {
        return Ok(vec![]);
    }
    let mut result = vec![];
    for (count, entry) in fs::read_dir(folder)?.enumerate() {
        if count >= 4096 {
            return Err(CoreError::InvalidInput);
        }
        let entry = entry?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if crate::instances::model::valid_id(&id).is_err() {
            continue;
        }
        let saved = transaction::load(paths, directory, &id);
        let compatible = saved.as_ref().is_ok_and(|s| {
            s.minecraft == instance.minecraft_version
                && s.loader == instance.loader
                && s.loader_version == instance.loader_version
        });
        result.push(crate::content::model::ContentRestorePoint {
            id,
            timestamp: saved.as_ref().map_or(0, |s| s.event.timestamp),
            title: instance.name.clone(),
            files: saved.as_ref().map_or(0, |s| s.before.len() as u32),
            available: compatible,
            settings_available: saved
                .as_ref()
                .is_ok_and(|s| s.before.keys().any(|k| transaction::is_setting(k))),
            project_point: true,
        });
    }
    Ok(result)
}
pub(crate) fn validate_launch(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
) -> Result<(), CoreError> {
    let Some(value) = manifest(paths, directory)? else {
        return Ok(());
    };
    compatible(&value, instance)?;
    for file in &value.files {
        let target = paths.checked(&directory.join(&file.path))?;
        match file.policy {
            FilePolicy::RequiredLocked
                if !target.exists() || transaction::digest(&target)? != file.blob() =>
            {
                return Err(CoreError::ContentConflict);
            }
            FilePolicy::UserForbidden if target.exists() => return Err(CoreError::ContentConflict),
            _ => (),
        }
    }
    if value.forbid_external_mods {
        for name in collect(paths, directory)? {
            if (name.starts_with("mods/") || name.starts_with("mods_disabled/"))
                && name.to_ascii_lowercase().ends_with(".jar")
                && !value
                    .files
                    .iter()
                    .any(|f| f.path.eq_ignore_ascii_case(&name))
            {
                return Err(CoreError::ContentConflict);
            }
        }
    }
    Ok(())
}
fn write_copy(
    paths: &Paths,
    source: &Path,
    target: &Path,
    blob: &transaction::Blob,
) -> Result<(), CoreError> {
    paths.checked(source)?;
    paths.checked(target)?;
    if transaction::digest(source)? != *blob {
        return Err(CoreError::SourceChanged);
    }
    paths.mkdir(target.parent().ok_or(CoreError::UnsafePath)?)?;
    use std::io::Write;
    let mut temp = tempfile::NamedTempFile::new_in(target.parent().ok_or(CoreError::UnsafePath)?)?;
    std::io::copy(&mut fs::File::open(source)?, &mut temp)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    if transaction::digest(temp.path())? != *blob {
        return Err(CoreError::Integrity);
    }
    if target.exists() {
        if transaction::digest(target)? == *blob {
            return Ok(());
        }
        return Err(CoreError::ContentConflict);
    }
    temp.persist_noclobber(target)
        .map_err(|e| CoreError::Io(e.error))?;
    Ok(())
}
