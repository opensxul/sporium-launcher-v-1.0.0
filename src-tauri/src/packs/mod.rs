pub(crate) mod archive;
mod external;
pub mod model;
#[cfg(test)]
mod tests;

use crate::{
    content::{install, model::*, modrinth::Modrinth, provider::ContentProvider, resolve},
    error::{CommandError, CoreError},
    game::{
        fs::write_atomic,
        network::{Download, Hash, Network, verify},
    },
    instances::{
        Library,
        filesystem::{Paths, no_links},
        model::CreateInstance,
    },
};
use model::*;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File},
    io::Write,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

struct Plan {
    created: Instant,
    view: PackPreview,
    manifest: Manifest,
    embedded: BTreeMap<String, EmbeddedFile>,
    tree: Stage,
}
struct Stage {
    tree: tempfile::TempDir,
    _lease: File,
}
pub(crate) struct PreparedProjectPack {
    pub game: std::path::PathBuf,
    pub manifest: Manifest,
    pub records: Vec<ContentRecord>,
    _stage: Stage,
}
impl Stage {
    fn path(&self) -> &Path {
        self.tree.path()
    }
}
fn prune(paths: &Paths, root: &Path) -> Result<(), CoreError> {
    for (count, entry) in fs::read_dir(root)?.enumerate() {
        if count > 1024 {
            return Err(CoreError::Integrity);
        }
        let entry = entry?;
        let path = paths.checked(&entry.path())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !path.is_dir() || !name.starts_with("pack-") || name.len() > 80 {
            continue;
        }
        let marker = paths.checked(&path.join(".pack-stage.json"))?;
        if !marker.is_file() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_slice(&crate::game::fs::read_limited(paths, &marker, 1024)?)?;
        if value.get("schema").and_then(serde_json::Value::as_u64) != Some(1)
            || value.get("name").and_then(serde_json::Value::as_str) != Some(name.as_str())
        {
            continue;
        }
        let lock = paths.checked(
            &paths
                .root()
                .join("launcher/pack-locks")
                .join(format!("{name}.lock")),
        )?;
        let lease = match paths.named_lock(&lock) {
            Ok(lease) => lease,
            Err(CoreError::LibraryBusy | CoreError::InstanceBusy) => continue,
            Err(e) => return Err(e),
        };
        paths.remove(&path)?;
        drop(lease);
        paths.remove(&lock)?;
    }
    Ok(())
}
#[derive(Default)]
struct State {
    plans: HashMap<String, Plan>,
    candidates: HashMap<String, (Instant, external::Candidate)>,
    job: Option<PackJob>,
    opening: Vec<String>,
}
#[derive(Clone)]
pub struct PackManager {
    library: Library,
    state: Arc<Mutex<State>>,
    cancel: Arc<AtomicBool>,
}
impl PackManager {
    pub fn new(library: Library) -> Self {
        // Arguments are file-open requests, never commands to execute. UI still requires confirmation.
        let opening = std::env::args()
            .skip(1)
            .filter(|arg| {
                Path::new(arg).is_absolute()
                    && matches!(
                        Path::new(arg)
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(str::to_ascii_lowercase)
                            .as_deref(),
                        Some("mrpack" | "sporium")
                    )
            })
            .take(8)
            .collect();
        Self {
            library,
            state: Arc::new(Mutex::new(State {
                opening,
                ..State::default()
            })),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn opening(&self) -> Vec<String> {
        std::mem::take(&mut self.state.lock().unwrap().opening)
    }
    pub fn snapshot(&self) -> Option<PackJob> {
        self.state.lock().unwrap().job.clone()
    }
    pub fn is_active(&self) -> bool {
        self.snapshot()
            .is_some_and(|j| matches!(j.phase.as_str(), "downloading" | "applying"))
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    fn stage(&self) -> Result<(Paths, Stage), CoreError> {
        let paths = Paths::new(self.library.root())?;
        let root = paths.checked(&paths.root().join("launcher/pack-staging"))?;
        paths.mkdir(&root)?;
        let _guard = paths.named_lock(&paths.root().join("launcher/pack-staging.lock"))?;
        prune(&paths, &root)?;
        let tree = tempfile::Builder::new().prefix("pack-").tempdir_in(root)?;
        let name = tree
            .path()
            .file_name()
            .ok_or(CoreError::UnsafePath)?
            .to_string_lossy()
            .to_string();
        let _lease = paths.named_lock(
            &paths
                .root()
                .join("launcher/pack-locks")
                .join(format!("{name}.lock")),
        )?;
        write_atomic(
            &paths,
            &tree.path().join(".pack-stage.json"),
            &serde_json::to_vec(&serde_json::json!({"schema":1,"name":name}))?,
        )?;
        Ok((paths, Stage { tree, _lease }))
    }
    fn retain(
        &self,
        source: &str,
        parsed: archive::Parsed,
        tree: Stage,
    ) -> Result<PackPreview, CoreError> {
        let (request, version) = archive::metadata(&parsed.manifest)?;
        let total = parsed
            .manifest
            .files
            .iter()
            .map(|f| f.file_size)
            .sum::<u64>()
            .checked_add(parsed.embedded.values().map(|f| f.size).sum())
            .ok_or(CoreError::Integrity)?;
        if total > archive::MAX_BYTES {
            return Err(CoreError::Integrity);
        }
        let mut required = 0;
        let mut optional = vec![];
        let mut bytes = 0;
        let mut skipped = 0;
        for file in &parsed.manifest.files {
            if parsed.embedded.contains_key(&file.path) {
                continue;
            }
            match file
                .env
                .as_ref()
                .map(|e| e.client.as_str())
                .unwrap_or("required")
            {
                "unsupported" => skipped += 1,
                "optional" => optional.push(file.path.clone()),
                _ => {
                    required += 1;
                    bytes += file.file_size;
                }
            }
        }
        let mut warnings = parsed.warnings;
        if skipped > 0 {
            warnings.push(format!("server_files_skipped:{skipped}"));
        }
        let view = PackPreview {
            token: uuid::Uuid::new_v4().to_string(),
            source: source.into(),
            name: request.name,
            version: parsed.manifest.version_id.clone(),
            description: parsed.manifest.summary.clone().unwrap_or_default(),
            minecraft: request.minecraft_version,
            loader: request.loader,
            loader_version: version,
            required_files: required,
            optional_files: optional,
            download_bytes: bytes,
            embedded_bytes: parsed.embedded.values().map(|v| v.size).sum(),
            warnings,
        };
        let mut state = self.state.lock().unwrap();
        state
            .plans
            .retain(|_, p| p.created.elapsed() < Duration::from_secs(900));
        if state.plans.len() >= 8 {
            return Err(CoreError::InstanceBusy);
        }
        state.plans.insert(
            view.token.clone(),
            Plan {
                created: Instant::now(),
                view: view.clone(),
                manifest: parsed.manifest,
                embedded: parsed.embedded,
                tree,
            },
        );
        Ok(view)
    }
    pub fn preview(&self, source: &Path) -> Result<PackPreview, CoreError> {
        let sporium = match source
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("sporium") => true,
            Some("mrpack") => false,
            _ => return Err(CoreError::ContentUnsupported),
        };
        let (paths, tree) = self.stage()?;
        // Copy/freeze the archive, so a replaced file cannot change a confirmed preview.
        no_links(source)?;
        if !source.is_absolute() || source.metadata()?.len() > archive::MAX_FILE {
            return Err(CoreError::UnsafeArchive);
        }
        let frozen = paths.checked(&tree.path().join("source.zip"))?;
        let before = archive::digest(source)?;
        fs::copy(source, &frozen)?;
        let copied = archive::digest(&frozen)?;
        let after = archive::digest(source)?;
        if before.sha512 != copied.sha512 || before.sha512 != after.sha512 {
            return Err(CoreError::SourceChanged);
        }
        let game = tree.path().join("game");
        paths.mkdir(&game)?;
        let parsed = archive::parse(&paths, &frozen, &game, sporium)?;
        self.retain(
            if sporium {
                "Sporium"
            } else {
                "Modrinth .mrpack"
            },
            parsed,
            tree,
        )
    }
    pub fn provider_preview(
        &self,
        project_id: &str,
        version_id: &str,
    ) -> Result<PackPreview, CoreError> {
        let provider = Modrinth::new(self.library.root().to_path_buf())?;
        let project = provider.project(project_id)?;
        let version = provider.version(version_id)?;
        if project.project_type != "modpack" || version.project_id != project.id {
            return Err(CoreError::ContentUnsupported);
        }
        let files: Vec<_> = version
            .files
            .iter()
            .filter(|f| f.filename.ends_with(".mrpack") && f.primary)
            .collect();
        if files.len() != 1 {
            return Err(CoreError::ContentUnsupported);
        }
        let file = files[0];
        if !archive::pack_url(&file.url)
            || file.size > archive::MAX_FILE
            || !archive::hash_valid(&file.hashes.sha512, 128)
            || !archive::hash_valid(&file.hashes.sha1, 40)
        {
            return Err(CoreError::Integrity);
        }
        let (paths, tree) = self.stage()?;
        let _lock = paths.named_lock(&paths.root().join("shared/cache/downloads.lock"))?;
        let cached = paths.checked(
            &paths
                .root()
                .join("shared/cache/modrinth/packs")
                .join(format!("{}.mrpack", file.hashes.sha512)),
        )?;
        Network::new()?.download(
            &paths,
            &Download {
                url: file.url.clone(),
                path: cached.clone(),
                hash: Hash::Sha512(file.hashes.sha512.clone()),
                size: file.size,
            },
            &|| false,
            &|_| {},
        )?;
        if !verify(&cached, &Hash::Sha1(file.hashes.sha1.clone()), file.size)? {
            return Err(CoreError::Integrity);
        }
        let game = tree.path().join("game");
        paths.mkdir(&game)?;
        let mut parsed = archive::parse(&paths, &cached, &game, false)?;
        // API identifiers are proven by the exact downloaded pack hashes, stored separately from archive declarations.
        parsed.warnings.push(format!(
            "modrinth_pack_ref:{}:{}:{}",
            project.id, version.id, file.hashes.sha512
        ));
        self.retain("Modrinth .mrpack", parsed, tree)
    }
    pub fn scan(&self, source: &Path) -> Result<Vec<ExternalCandidate>, CoreError> {
        let candidates = external::scan(source)?;
        let mut state = self.state.lock().unwrap();
        state
            .candidates
            .retain(|_, (t, _)| t.elapsed() < Duration::from_secs(900));
        if state.candidates.len() + candidates.len() > 500 {
            state.candidates.clear();
        }
        let views = candidates.iter().map(|c| c.view.clone()).collect();
        for candidate in candidates {
            state
                .candidates
                .insert(candidate.view.key.clone(), (Instant::now(), candidate));
        }
        Ok(views)
    }
    pub fn external_preview(&self, key: &str) -> Result<PackPreview, CoreError> {
        let (time, candidate) = self
            .state
            .lock()
            .unwrap()
            .candidates
            .get(key)
            .cloned()
            .ok_or(CoreError::NotFound)?;
        if time.elapsed() > Duration::from_secs(900) {
            return Err(CoreError::RecordConflict);
        }
        external::verify_candidate(&candidate)?;
        let (paths, tree) = self.stage()?;
        let game = tree.path().join("game");
        paths.mkdir(&game)?;
        let (embedded, omitted) = external::copy(&paths, &candidate.game, &game)?;
        external::verify_candidate(&candidate)?;
        let mut warnings = candidate.view.warnings.clone();
        warnings.extend(
            omitted
                .into_iter()
                .map(|n| format!("source_item_skipped:{n}")),
        );
        let manifest = external::manifest(&candidate.view);
        self.retain(
            &candidate.view.source,
            archive::Parsed {
                manifest,
                embedded,
                warnings,
            },
            tree,
        )
    }
    pub fn dismiss(&self, token: &str) {
        self.state.lock().unwrap().plans.remove(token);
    }
    pub fn start(
        &self,
        token: &str,
        name: &str,
        optional: Vec<String>,
    ) -> Result<PackJob, CoreError> {
        let mut state = self.state.lock().unwrap();
        if state
            .job
            .as_ref()
            .is_some_and(|j| matches!(j.phase.as_str(), "downloading" | "applying"))
        {
            return Err(CoreError::InstanceBusy);
        }
        let plan = state.plans.get(token).ok_or(CoreError::NotFound)?;
        if plan.created.elapsed() > Duration::from_secs(900) {
            return Err(CoreError::RecordConflict);
        }
        let (mut request, version) = archive::metadata(&plan.manifest)?;
        request.name = name.into();
        Library::draft(&request)?;
        let chosen: HashSet<_> = optional.into_iter().collect();
        if chosen.iter().any(|n| !plan.view.optional_files.contains(n)) {
            return Err(CoreError::InvalidInput);
        }
        let files: Vec<_> = plan
            .manifest
            .files
            .iter()
            .filter(|f| {
                !plan.embedded.contains_key(&f.path)
                    && match f
                        .env
                        .as_ref()
                        .map(|e| e.client.as_str())
                        .unwrap_or("required")
                    {
                        "unsupported" => false,
                        "optional" => chosen.contains(&f.path),
                        _ => true,
                    }
            })
            .cloned()
            .collect();
        let paths = Paths::new(self.library.root())?;
        let lock = paths.named_lock(&paths.root().join("shared/cache/downloads.lock"))?;
        let job = PackJob {
            name: name.into(),
            phase: "downloading".into(),
            instance_id: None,
            completed_files: 0,
            total_files: files.len() as u32,
            total_bytes: files.iter().map(|f| f.file_size).sum(),
            downloaded_bytes: 0,
            error: None,
        };
        let plan = state.plans.remove(token).ok_or(CoreError::NotFound)?;
        state.job = Some(job.clone());
        self.cancel.store(false, Ordering::SeqCst);
        drop(state);
        let manager = self.clone();
        std::thread::Builder::new()
            .name("sporium-pack-import".into())
            .spawn(move || {
                let _lock = lock;
                let result = manager.install(&paths, &plan, request, version, &files, true);
                let mut state = manager.state.lock().unwrap();
                if let Some(job) = &mut state.job {
                    match result {
                        Ok(id) => {
                            job.phase = "completed".into();
                            job.instance_id = Some(id);
                            job.downloaded_bytes = job.total_bytes;
                        }
                        Err(CoreError::Cancelled) => job.phase = "cancelled".into(),
                        Err(error) => {
                            job.phase = "failed".into();
                            job.error = Some(CommandError::from(error));
                        }
                    }
                }
            })
            .map_err(|_| {
                if let Some(job) = &mut self.state.lock().unwrap().job {
                    job.phase = "failed".into();
                    job.error = Some(CommandError::from(CoreError::Worker));
                }
                CoreError::Worker
            })?;
        Ok(job)
    }
    fn install(
        &self,
        paths: &Paths,
        plan: &Plan,
        request: CreateInstance,
        version: Option<String>,
        files: &[PackFile],
        publish: bool,
    ) -> Result<String, CoreError> {
        let network = Network::new()?;
        let game = plan.tree.path().join("game");
        for (name, file) in &plan.embedded {
            if !verify(
                &paths.checked(&game.join(name))?,
                &Hash::Sha512(file.sha512.clone()),
                file.size,
            )? {
                return Err(CoreError::SourceChanged);
            }
        }
        for file in files {
            if self.cancel.load(Ordering::SeqCst) {
                return Err(CoreError::Cancelled);
            }
            let sha512 = file.hashes.get("sha512").ok_or(CoreError::Integrity)?;
            let cached = paths.checked(
                &paths
                    .root()
                    .join("shared/cache/modrinth/artifacts")
                    .join(format!("{sha512}.bin")),
            )?;
            let mut last = CoreError::Network;
            let mut downloaded = false;
            for url in &file.downloads {
                let result = network.download(
                    paths,
                    &Download {
                        url: url.clone(),
                        path: cached.clone(),
                        hash: Hash::Sha512(sha512.clone()),
                        size: file.file_size,
                    },
                    &|| self.cancel.load(Ordering::SeqCst),
                    &|bytes| {
                        if let Some(job) = &mut self.state.lock().unwrap().job {
                            job.downloaded_bytes = job
                                .downloaded_bytes
                                .saturating_add(bytes)
                                .min(job.total_bytes);
                        }
                    },
                );
                match result {
                    Ok(_) => {
                        downloaded = true;
                        break;
                    }
                    Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
                    Err(e) => last = e,
                }
            }
            if !downloaded {
                return Err(last);
            }
            if !verify(
                &cached,
                &Hash::Sha1(file.hashes.get("sha1").ok_or(CoreError::Integrity)?.clone()),
                file.file_size,
            )? {
                return Err(CoreError::Integrity);
            }
            let target = paths.checked(
                &game.join(
                    archive::allowed_path(&file.path)
                        .and_then(|_| crate::game::fs::relative(&file.path))?,
                ),
            )?;
            paths.mkdir(target.parent().ok_or(CoreError::UnsafePath)?)?;
            fs::copy(&cached, &target)?;
            if !verify(&target, &Hash::Sha512(sha512.clone()), file.file_size)? {
                return Err(CoreError::Integrity);
            }
            fs::OpenOptions::new()
                .write(true)
                .open(target)?
                .sync_all()?;
            if let Some(job) = &mut self.state.lock().unwrap().job {
                job.completed_files += 1;
            }
        }
        if self.cancel.load(Ordering::SeqCst) {
            return Err(CoreError::Cancelled);
        }
        if let Some(job) = &mut self.state.lock().unwrap().job {
            job.phase = "applying".into();
        }
        // Provider provenance requires the API's exact hash/file mapping. The archive cannot forge receipts.
        let mut instance = Library::draft(&request)?;
        instance.loader_version = version.clone();
        let provider = Modrinth::new(self.library.root().to_path_buf())?;
        let records = receipts(paths, &game, &instance, &provider, &self.cancel)?;
        install::write_records(paths, &game, &records)?;
        write_atomic(
            paths,
            &game.join(".sporium/import.json"),
            &serde_json::to_vec(
                &serde_json::json!({"schemaVersion":1,"source":plan.view.source,"manifest":plan.manifest,"selectedFiles":files,"embedded":plan.embedded,"providerRefs":plan.view.warnings.iter().filter(|w|w.starts_with("modrinth_pack_ref:")).collect::<Vec<_>>(),"importedAt":crate::instances::model::now()}),
            )?,
        )?;
        if self.cancel.load(Ordering::SeqCst) {
            return Err(CoreError::Cancelled);
        }
        if !publish {
            return Ok(game.to_string_lossy().into_owned());
        }
        let change = self.library.import_prepared(
            request,
            version,
            &game,
            format!("pack:{}:{}", plan.view.source, plan.manifest.version_id),
        )?;
        Ok(change.affected_id)
    }
    pub(crate) fn prepare_existing(
        &self,
        token: &str,
        instance: &crate::instances::model::Instance,
        optional: &[String],
    ) -> Result<PreparedProjectPack, CoreError> {
        let plan = self
            .state
            .lock()
            .unwrap()
            .plans
            .remove(token)
            .ok_or(CoreError::NotFound)?;
        if plan.created.elapsed() > Duration::from_secs(900) {
            return Err(CoreError::RecordConflict);
        }
        let (request, version) = archive::metadata(&plan.manifest)?;
        if request.minecraft_version != instance.minecraft_version
            || request.loader != instance.loader
            || version != instance.loader_version
        {
            return Err(CoreError::ContentIncompatible);
        }
        let files: Vec<_> = plan
            .manifest
            .files
            .iter()
            .filter(|f| {
                !plan.embedded.contains_key(&f.path)
                    && match f
                        .env
                        .as_ref()
                        .map(|e| e.client.as_str())
                        .unwrap_or("required")
                    {
                        "unsupported" => false,
                        "optional" => optional.contains(&f.path),
                        _ => true,
                    }
            })
            .cloned()
            .collect();
        let paths = Paths::new(self.library.root())?;
        let _lock = paths.named_lock(&paths.root().join("shared/cache/downloads.lock"))?;
        self.cancel.store(false, Ordering::SeqCst);
        self.install(&paths, &plan, request, version, &files, false)?;
        let game = plan.tree.path().join("game");
        let records = install::records(&paths, &game)?;
        Ok(PreparedProjectPack {
            game,
            manifest: plan.manifest,
            records,
            _stage: plan.tree,
        })
    }
    pub fn export(
        &self,
        request: PackExportRequest,
        destination: &Path,
    ) -> Result<PackExport, CoreError> {
        no_links(destination)?;
        if !destination.is_absolute()
            || destination.starts_with(self.library.root())
            || destination.extension().and_then(|s| s.to_str()) != Some("sporium")
        {
            return Err(CoreError::UnsafePath);
        }
        let (instance, directory, _lease) = self.library.lease_game(&request.id)?;
        let paths = Paths::new(self.library.root())?;
        let records = install::records(&paths, &directory)?;
        let candidate = ExternalCandidate {
            key: String::new(),
            name: instance.name.clone(),
            minecraft: instance.minecraft_version.clone(),
            loader: instance.loader,
            loader_version: instance.loader_version.clone(),
            source: "Sporium".into(),
            warnings: vec![],
        };
        let mut manifest = external::manifest(&candidate);
        manifest.version_id = format!("sporium-{}", crate::instances::model::now());
        let (files, mut omitted) = external::files(&directory, request.include_worlds)?;
        let parent = destination.parent().ok_or(CoreError::UnsafePath)?;
        no_links(parent)?;
        if !parent.is_dir() {
            return Err(CoreError::NotFound);
        }
        let temporary = tempfile::NamedTempFile::new_in(parent)?;
        let mut zip = zip::ZipWriter::new(temporary.reopen()?);
        let mut embedded = BTreeMap::new();
        for (name, path) in files {
            let digest = archive::digest(&path)?;
            if let Some(record) = records.iter().find(|r| {
                format!("{}/{}", r.directory, r.file.filename) == name
                    && r.provider == "modrinth"
                    && archive::pack_url(&r.file.url)
                    && r.file.size == digest.size
                    && r.file.hashes.sha512.eq_ignore_ascii_case(&digest.sha512)
            }) {
                if !verify(
                    &path,
                    &Hash::Sha1(record.file.hashes.sha1.clone()),
                    record.file.size,
                )? {
                    return Err(CoreError::SourceChanged);
                }
                manifest.files.push(PackFile {
                    path: name,
                    hashes: BTreeMap::from([
                        ("sha1".into(), record.file.hashes.sha1.clone()),
                        ("sha512".into(), record.file.hashes.sha512.clone()),
                    ]),
                    downloads: vec![record.file.url.clone()],
                    file_size: record.file.size,
                    env: None,
                });
            } else {
                let binary = matches!(
                    name.split('/').next(),
                    Some("mods" | "mods_disabled" | "resourcepacks" | "shaderpacks")
                );
                if binary && !request.include_local {
                    omitted.push(name);
                    continue;
                }
                archive::zip_file(&mut zip, &format!("overrides/{name}"), &path)?;
                let after = archive::digest(&path)?;
                if after.sha512 != digest.sha512 || after.size != digest.size {
                    return Err(CoreError::SourceChanged);
                }
                embedded.insert(name, digest);
            }
        }
        let pack = SporiumPack {
            schema_version: 1,
            manifest,
            overrides: embedded,
        };
        archive::metadata(&pack.manifest)?;
        zip.start_file(
            "sporium.index.json",
            zip::write::SimpleFileOptions::default(),
        )
        .map_err(|_| CoreError::Integrity)?;
        zip.write_all(&serde_json::to_vec(&pack)?)?;
        zip.finish().map_err(|_| CoreError::Integrity)?.sync_all()?;
        if temporary.as_file().metadata()?.len() > archive::MAX_FILE {
            return Err(CoreError::Integrity);
        }
        no_links(destination)?;
        temporary
            .persist_noclobber(destination)
            .map_err(|e| CoreError::Io(e.error))?;
        Ok(PackExport {
            file_name: destination
                .file_name()
                .ok_or(CoreError::UnsafePath)?
                .to_string_lossy()
                .into_owned(),
            referenced_files: pack.manifest.files.len() as u32,
            embedded_files: pack.overrides.len() as u32,
            omitted,
        })
    }
}

pub(crate) fn receipts(
    paths: &Paths,
    game: &Path,
    instance: &crate::instances::model::Instance,
    provider: &dyn ContentProvider,
    cancel: &AtomicBool,
) -> Result<Vec<ContentRecord>, CoreError> {
    let (files, _) = external::files(game, true)?;
    let mut records = vec![];
    for (name, path) in files {
        paths.checked(&path)?;
        if cancel.load(Ordering::SeqCst) {
            return Err(CoreError::Cancelled);
        }
        let Some((directory, filename)) = name.split_once('/') else {
            continue;
        };
        let kind = match directory {
            "mods" | "mods_disabled" if !filename.contains('/') && filename.ends_with(".jar") => {
                "mod"
            }
            "resourcepacks" if !filename.contains('/') => "resourcepack",
            "shaderpacks" if !filename.contains('/') => "shader",
            _ => continue,
        };
        let hash = archive::digest(&path)?;
        let Ok(Some(version)) = provider.version_from_hash(&hash.sha512) else {
            continue;
        };
        let Some(file) = version
            .files
            .iter()
            .find(|f| f.size == hash.size && f.hashes.sha512.eq_ignore_ascii_case(&hash.sha512))
        else {
            continue;
        };
        if resolve::validate_file(file, directory).is_err()
            || !verify(&path, &Hash::Sha1(file.hashes.sha1.clone()), file.size)?
        {
            continue;
        }
        let Ok(project) = provider.project(&version.project_id) else {
            continue;
        };
        if project.project_type != kind || !resolve::compatible(&project, &version, instance) {
            continue;
        }
        let mut file = file.clone();
        file.filename = filename.into();
        let record = ContentRecord {
            local: None,
            icon_url: project.icon_url,
            provider: "modrinth".into(),
            project_id: project.id,
            title: project.title,
            kind: kind.into(),
            version,
            file,
            directory: directory.into(),
            dependency: false,
        };
        records.push(record);
        if records.len() > 4096 {
            return Err(CoreError::Integrity);
        }
    }
    Ok(records)
}
