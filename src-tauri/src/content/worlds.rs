//! Local world import and explicit per-world datapack destinations.
use super::{install, manage, model::*, provider::ContentProvider, resolve};
use crate::{
    error::CoreError,
    game::{
        fs::{read_limited, relative, write_atomic},
        network::{Hash, hex, verify},
    },
    instances::{
        filesystem::Paths,
        model::{Instance, now, valid_id},
    },
};
use serde::{Deserialize, Serialize};
use sha1::Digest;
use std::{
    collections::{HashSet, VecDeque},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

pub(super) fn world_id(id: &str) -> Result<(), CoreError> {
    if id.len() > 180 || relative(id)?.components().count() != 1 {
        return Err(CoreError::UnsafePath);
    }
    Ok(())
}
pub(crate) fn datapack_directory(folder: &str) -> bool {
    let parts: Vec<_> = folder.split('/').collect();
    parts.len() == 3 && parts[0] == "saves" && parts[2] == "datapacks" && world_id(parts[1]).is_ok()
}
pub(super) fn world(paths: &Paths, directory: &Path, id: &str) -> Result<PathBuf, CoreError> {
    world_id(id)?;
    let root = paths.checked(&directory.join("saves").join(id))?;
    let level = paths.checked(&root.join("level.dat"))?;
    if !root.is_dir() || !level.is_file() || level.metadata()?.len() == 0 {
        return Err(CoreError::NotFound);
    }
    Ok(root)
}
pub(crate) fn validate_destination(
    paths: &Paths,
    directory: &Path,
    folder: &str,
) -> Result<(), CoreError> {
    if datapack_directory(folder) {
        world(
            paths,
            directory,
            folder.split('/').nth(1).ok_or(CoreError::UnsafePath)?,
        )?;
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Imports {
    schema: u32,
    worlds: Vec<WorldImport>,
}
fn imports(paths: &Paths, directory: &Path) -> Result<Vec<WorldImport>, CoreError> {
    let file = paths.checked(&directory.join(".sporium/world-imports.json"))?;
    if !file.exists() {
        return Ok(vec![]);
    }
    let value: Imports = serde_json::from_slice(&read_limited(paths, &file, 2_000_000)?)?;
    if value.schema != 1 {
        return Err(CoreError::SchemaTooNew);
    }
    if value.worlds.len() > 4096 {
        return Err(CoreError::Integrity);
    }
    for item in &value.worlds {
        world_id(&item.id)?;
    }
    Ok(value.worlds)
}
pub(super) fn list(paths: &Paths, directory: &Path) -> Result<Vec<ContentWorld>, CoreError> {
    let root = paths.checked(&directory.join("saves"))?;
    if !root.exists() {
        return Ok(vec![]);
    }
    let known = imports(paths, directory)?;
    let mut result = vec![];
    for (count, entry) in fs::read_dir(root)?.enumerate() {
        if count >= 4096 {
            return Err(CoreError::Integrity);
        }
        let entry = entry?;
        paths.checked(&entry.path())?;
        let id = entry.file_name().to_string_lossy().to_string();
        if world(paths, directory, &id).is_err() {
            continue;
        }
        let imported = known.iter().find(|w| w.id == id).cloned();
        result.push(ContentWorld {
            title: imported
                .as_ref()
                .map(|w| w.title.clone())
                .unwrap_or_else(|| id.clone()),
            id,
            imported,
        });
    }
    result.sort_by_key(|w| w.title.to_lowercase());
    Ok(result)
}
pub(super) fn artifact(file: &Path, name: &str) -> Result<ContentFile, CoreError> {
    let mut input = File::open(file)?;
    let size = input.metadata()?.len();
    if size == 0 || size > 500_000_000 {
        return Err(CoreError::Integrity);
    }
    let mut sha1 = sha1::Sha1::new();
    let mut sha512 = sha2::Sha512::new();
    let mut buffer = [0; 65536];
    let mut read = 0;
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        read += n as u64;
        if read > size {
            return Err(CoreError::SourceChanged);
        }
        sha1.update(&buffer[..n]);
        sha512.update(&buffer[..n]);
    }
    if read != size {
        return Err(CoreError::SourceChanged);
    }
    Ok(ContentFile {
        id: None,
        filename: name.into(),
        url: String::new(),
        hashes: ContentHashes {
            sha1: hex(&sha1.finalize()),
            sha512: hex(&sha512.finalize()),
        },
        size,
        primary: true,
        file_type: None,
    })
}
// Full path preflight happens before any extraction. Reject links, duplicate Windows paths and bombs.
fn entries(zip: &mut zip::ZipArchive<File>) -> Result<Vec<String>, CoreError> {
    if zip.len() > 10000 {
        return Err(CoreError::UnsafeArchive);
    }
    let mut names = vec![];
    let mut seen = HashSet::new();
    let mut expanded = 0u64;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|_| CoreError::Integrity)?;
        let part = relative(entry.name().trim_end_matches('/'))?;
        if part.as_os_str().is_empty()
            || entry.name().contains('\\')
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(CoreError::UnsafeArchive);
        }
        let name = entry.name().trim_end_matches('/').to_string();
        if name.len() > 240 || !seen.insert(name.to_lowercase()) {
            return Err(CoreError::UnsafeArchive);
        }
        expanded = expanded
            .checked_add(entry.size())
            .ok_or(CoreError::UnsafeArchive)?;
        if entry.size() > 500_000_000 || expanded > 2_000_000_000 {
            return Err(CoreError::UnsafeArchive);
        }
        names.push(if entry.is_dir() {
            format!("{name}/")
        } else {
            name
        });
    }
    Ok(names)
}
fn pack_format(value: &serde_json::Value) -> Option<(u64, u64)> {
    let pair = if let Some(major) = value.as_u64() {
        (major, 0)
    } else {
        let parts = value.as_array()?;
        if parts.len() != 2 {
            return None;
        }
        (parts[0].as_u64()?, parts[1].as_u64()?)
    };
    (pair.0 > 0 && pair.0 <= 10000 && pair.1 <= 10000).then_some(pair)
}
pub(super) fn validate_datapack(file: &Path) -> Result<(), CoreError> {
    let mut zip = zip::ZipArchive::new(File::open(file)?).map_err(|_| CoreError::Integrity)?;
    let names = entries(&mut zip)?;
    if !names
        .iter()
        .any(|n| n.starts_with("data/") && !n.ends_with('/'))
    {
        return Err(CoreError::ContentUnsupported);
    }
    let entry = zip
        .by_name("pack.mcmeta")
        .map_err(|_| CoreError::ContentUnsupported)?;
    if entry.size() > 262144 {
        return Err(CoreError::Integrity);
    }
    let mut bytes = vec![];
    entry.take(262145).read_to_end(&mut bytes)?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| CoreError::Integrity)?;
    let pack = value
        .get("pack")
        .and_then(|p| p.as_object())
        .ok_or(CoreError::ContentUnsupported)?;
    if !pack
        .get("pack_format")
        .is_some_and(|v| v.as_u64().is_some_and(|n| n > 0 && n <= 10000))
        && !pack
            .get("min_format")
            .and_then(pack_format)
            .zip(pack.get("max_format").and_then(pack_format))
            .is_some_and(|(min, max)| min <= max)
    {
        return Err(CoreError::ContentUnsupported);
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TreeFile {
    name: String,
    sha512: String,
    size: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapJournal {
    schema: u32,
    token: String,
    imported: WorldImport,
    files: Vec<TreeFile>,
}
pub(super) struct Prepared {
    pub plan: WorldArchivePlan,
    pub record: Option<ContentRecord>,
    files: Vec<TreeFile>,
    source_artifact: ContentFile,
    world_marker: Option<String>,
}
pub(super) fn marker(paths: &Paths, directory: &Path, id: &str) -> Result<String, CoreError> {
    let file = world(paths, directory, id)?.join("level.dat");
    if file.metadata()?.len() > 32_000_000 {
        return Err(CoreError::Integrity);
    }
    digest(&file)
}
fn stage_root(paths: &Paths, directory: &Path, token: &str) -> Result<PathBuf, CoreError> {
    valid_id(token)?;
    paths.checked(&directory.join(".sporium/world-staging").join(token))
}
pub(super) fn prepare(
    paths: &Paths,
    directory: &Path,
    request: WorldArchiveRequest,
) -> Result<Prepared, CoreError> {
    if !matches!(request.kind.as_str(), "map" | "datapack") {
        return Err(CoreError::InvalidInput);
    }
    let source = Path::new(&request.source);
    crate::instances::filesystem::no_links(source)?;
    if !source.is_absolute()
        || !source
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("zip"))
        || fs::symlink_metadata(source)?.file_type().is_symlink()
    {
        return Err(CoreError::InvalidInput);
    }
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(CoreError::InvalidInput)?
        .to_string();
    let title = request.title.trim().to_string();
    if title.is_empty() || title.chars().count() > 80 || title.chars().any(char::is_control) {
        return Err(CoreError::InvalidInput);
    }
    let token = uuid::Uuid::new_v4().to_string();
    let root = stage_root(paths, directory, &token)?;
    paths.mkdir(&root)?;
    let result = (|| {
        let staged = root.join("archive.zip");
        let mut input = File::open(source)?;
        let size = input.metadata()?.len();
        if size == 0 || size > 500_000_000 {
            return Err(CoreError::Integrity);
        }
        let mut output = File::create(&staged)?;
        if std::io::copy(&mut Read::by_ref(&mut input).take(500_000_001), &mut output)? != size {
            return Err(CoreError::SourceChanged);
        }
        output.sync_all()?;
        let file = artifact(&staged, &name)?;
        let mut plan = WorldArchivePlan {
            token: token.clone(),
            instance_id: request.instance_id,
            kind: request.kind.clone(),
            title: title.clone(),
            world: String::new(),
            source: name,
            bytes: size,
            files: 1,
            warnings: vec!["world_compatibility_unknown".into()],
        };
        if request.kind == "datapack" {
            let id = request.world.ok_or(CoreError::InvalidInput)?;
            world(paths, directory, &id)?;
            validate_datapack(&staged)?;
            plan.world = id.clone();
            let record = ContentRecord {
                local: Some(LocalMetadata::default()),
                icon_url: None,
                provider: "local".into(),
                project_id: token.clone(),
                title,
                kind: "datapack".into(),
                version: ContentVersion {
                    id: token.clone(),
                    project_id: token.clone(),
                    name: file.filename.clone(),
                    version_number: "?".into(),
                    version_type: "local".into(),
                    date_published: String::new(),
                    status: "local".into(),
                    game_versions: vec![],
                    loaders: vec!["datapack".into()],
                    environment: "unknown".into(),
                    dependencies: vec![],
                    files: vec![file.clone()],
                },
                file: file.clone(),
                directory: format!("saves/{id}/datapacks"),
                dependency: false,
            };
            install::preflight(
                paths,
                directory,
                std::slice::from_ref(&record),
                &install::records(paths, directory)?,
            )?;
            Ok(Prepared {
                plan,
                record: Some(record),
                files: vec![],
                source_artifact: file,
                world_marker: Some(marker(paths, directory, &id)?),
            })
        } else {
            let mut zip =
                zip::ZipArchive::new(File::open(&staged)?).map_err(|_| CoreError::Integrity)?;
            let names = entries(&mut zip)?;
            let levels: Vec<_> = names
                .iter()
                .filter(|n| n.as_str() == "level.dat" || n.ends_with("/level.dat"))
                .collect();
            if levels.len() != 1 {
                return Err(CoreError::ContentUnsupported);
            }
            let prefix = levels[0]
                .strip_suffix("level.dat")
                .ok_or(CoreError::Integrity)?;
            if !prefix.is_empty()
                && relative(prefix.trim_end_matches('/'))?.components().count() != 1
            {
                return Err(CoreError::ContentUnsupported);
            }
            if names.iter().any(|n| {
                !n.starts_with(prefix) && n.trim_end_matches('/') != prefix.trim_end_matches('/')
            }) {
                return Err(CoreError::ContentUnsupported);
            }
            let destination = root.join("world");
            paths.mkdir(&destination)?;
            let mut files = vec![];
            for (i, name) in names.iter().enumerate() {
                let Some(name) = name.strip_prefix(prefix) else {
                    continue;
                };
                if name.is_empty() {
                    continue;
                }
                let target =
                    paths.checked(&destination.join(relative(name.trim_end_matches('/'))?))?;
                if name.ends_with('/') {
                    paths.mkdir(&target)?;
                    continue;
                }
                paths.mkdir(target.parent().ok_or(CoreError::UnsafePath)?)?;
                let mut entry = zip.by_index(i).map_err(|_| CoreError::Integrity)?;
                let expected = entry.size();
                let mut output = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&target)?;
                if std::io::copy(&mut entry.by_ref().take(expected + 1), &mut output)? != expected {
                    return Err(CoreError::Integrity);
                }
                output.sync_all()?;
                files.push(TreeFile {
                    name: name.into(),
                    size: expected,
                    sha512: digest(&target)?,
                });
            }
            if fs::metadata(destination.join("level.dat"))?.len() == 0 {
                return Err(CoreError::Integrity);
            }
            plan.world = format!("Sporium-{token}");
            plan.files = files.len() as u32;
            Ok(Prepared {
                plan,
                record: None,
                files,
                source_artifact: file,
                world_marker: None,
            })
        }
    })();
    if result.is_err() {
        let _ = paths.remove(&root);
    }
    result
}
fn digest(file: &Path) -> Result<String, CoreError> {
    let mut input = File::open(file)?;
    let mut hash = sha2::Sha512::new();
    let mut bytes = [0; 65536];
    loop {
        let n = input.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    Ok(hex(&hash.finalize()))
}
fn verify_tree(paths: &Paths, root: &Path, files: &[TreeFile]) -> Result<(), CoreError> {
    if files.is_empty() || files.len() > 10000 {
        return Err(CoreError::Integrity);
    }
    let mut names = HashSet::new();
    for file in files {
        let target = paths.checked(&root.join(relative(&file.name)?))?;
        if !names.insert(file.name.to_lowercase())
            || !verify(&target, &Hash::Sha512(file.sha512.clone()), file.size)?
        {
            return Err(CoreError::Integrity);
        }
    }
    // Reject files introduced after preview; no unexpected bytes can be published.
    let mut pending = vec![root.to_path_buf()];
    let mut count = 0;
    let mut visited = 0;
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(folder)? {
            let entry = entry?;
            visited += 1;
            if visited > 20000 {
                return Err(CoreError::Integrity);
            }
            let file = paths.checked(&entry.path())?;
            if file.is_dir() {
                pending.push(file);
            } else if file.is_file() {
                count += 1;
                let name = file
                    .strip_prefix(root)
                    .map_err(|_| CoreError::UnsafePath)?
                    .to_string_lossy()
                    .replace('\\', "/")
                    .to_lowercase();
                if !names.contains(&name) {
                    return Err(CoreError::Integrity);
                }
            }
            if count > 10000 {
                return Err(CoreError::Integrity);
            }
        }
    }
    if count != files.len() {
        return Err(CoreError::Integrity);
    }
    Ok(())
}
pub(super) fn finish(
    paths: &Paths,
    directory: &Path,
    prepared: &Prepared,
) -> Result<(), CoreError> {
    let root = stage_root(paths, directory, &prepared.plan.token)?;
    if !install::verified(&root.join("archive.zip"), &prepared.source_artifact)? {
        return Err(CoreError::Integrity);
    }
    if let Some(hash) = &prepared.world_marker
        && marker(paths, directory, &prepared.plan.world)? != *hash
    {
        return Err(CoreError::RecordConflict);
    }
    if let Some(record) = &prepared.record {
        world(paths, directory, &prepared.plan.world)?;
        if !install::verified(&root.join("archive.zip"), &record.file)? {
            return Err(CoreError::Integrity);
        }
        validate_datapack(&root.join("archive.zip"))?;
        let stage = install::stage(paths, directory, &prepared.plan.token, 0)?;
        paths.mkdir(stage.parent().ok_or(CoreError::UnsafePath)?)?;
        fs::copy(root.join("archive.zip"), stage)?;
        install::commit(
            paths,
            directory,
            &ContentPlan {
                token: prepared.plan.token.clone(),
                instance_id: prepared.plan.instance_id.clone(),
                new_instance: None,
                files: vec![record.clone()],
                optional_dependencies: 0,
                already_installed: 0,
                total_bytes: record.file.size,
            },
        )?;
    } else {
        let target = paths.checked(&directory.join("saves").join(&prepared.plan.world))?;
        if target.exists() {
            return Err(CoreError::ContentConflict);
        }
        verify_tree(paths, &root.join("world"), &prepared.files)?;
        let file = artifact(&root.join("archive.zip"), &prepared.plan.source)?;
        let journal = directory.join(".sporium/world-import.json");
        if paths.checked(&journal)?.exists() {
            return Err(CoreError::ContentConflict);
        }
        write_atomic(
            paths,
            &journal,
            &serde_json::to_vec(&MapJournal {
                schema: 1,
                token: prepared.plan.token.clone(),
                imported: WorldImport {
                    id: prepared.plan.world.clone(),
                    title: prepared.plan.title.clone(),
                    source: prepared.plan.source.clone(),
                    sha512: file.hashes.sha512,
                    imported_at: now(),
                },
                files: prepared.files.clone(),
            })?,
        )?;
        recover(paths, directory)?;
    }
    paths.remove(&root)?;
    Ok(())
}
pub(super) fn cancel(paths: &Paths, directory: &Path, token: &str) -> Result<(), CoreError> {
    paths.remove(&stage_root(paths, directory, token)?)
}
pub(super) fn recover(paths: &Paths, directory: &Path) -> Result<(), CoreError> {
    let file = paths.checked(&directory.join(".sporium/world-import.json"))?;
    if !file.exists() {
        return Ok(());
    }
    let journal: MapJournal = serde_json::from_slice(&read_limited(paths, &file, 8_000_000)?)?;
    if journal.schema != 1 {
        return Err(CoreError::SchemaTooNew);
    }
    world_id(&journal.imported.id)?;
    valid_id(&journal.token)?;
    if journal.imported.id != format!("Sporium-{}", journal.token) {
        return Err(CoreError::Integrity);
    }
    let root = stage_root(paths, directory, &journal.token)?;
    let target = paths.checked(&directory.join("saves").join(&journal.imported.id))?;
    if target.exists() {
        verify_tree(paths, &target, &journal.files)?;
    } else {
        verify_tree(paths, &root.join("world"), &journal.files)?;
        paths.mkdir(target.parent().ok_or(CoreError::UnsafePath)?)?;
        fs::rename(root.join("world"), &target)?;
    }
    world(paths, directory, &journal.imported.id)?;
    let mut known = imports(paths, directory)?;
    if !known.iter().any(|w| w.id == journal.imported.id) {
        known.push(journal.imported.clone());
    }
    if known.len() > 4096 {
        return Err(CoreError::Integrity);
    }
    write_atomic(
        paths,
        &directory.join(".sporium/world-imports.json"),
        &serde_json::to_vec(&Imports {
            schema: 1,
            worlds: known,
        })?,
    )?;
    manage::append_history(
        paths,
        directory,
        ContentHistory {
            id: journal.token.clone(),
            timestamp: journal.imported.imported_at,
            action: "import_world".into(),
            titles: vec![journal.imported.title],
        },
    )?;
    paths.remove(&file)?;
    paths.remove(&root)?;
    Ok(())
}
pub(super) fn is_datapack(project: &ContentProject, version: &ContentVersion) -> bool {
    matches!(project.project_type.as_str(), "mod" | "datapack")
        && version.loaders.iter().any(|s| s == "datapack")
}
pub(super) fn compatible(
    project: &ContentProject,
    version: &ContentVersion,
    instance: &Instance,
) -> bool {
    is_datapack(project, version)
        && version.project_id == project.id
        && version.game_versions.contains(&instance.minecraft_version)
        && matches!(version.status.as_str(), "listed" | "archived" | "unlisted")
}
pub(super) fn selected(
    project: &ContentProject,
    version: &ContentVersion,
    world: &str,
    dependency: bool,
) -> Result<Vec<ContentRecord>, CoreError> {
    world_id(world)?;
    let file = version
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| version.files.first())
        .ok_or(CoreError::Integrity)?;
    if matches!(
        file.file_type.as_deref(),
        Some("sources-jar" | "dev-jar" | "signature" | "javadoc-jar")
    ) {
        return Err(CoreError::ContentUnsupported);
    }
    resolve::validate_file(file, &format!("saves/{world}/datapacks"))?;
    let mut result = vec![ContentRecord {
        local: None,
        icon_url: project.icon_url.clone(),
        provider: "modrinth".into(),
        project_id: project.id.clone(),
        title: project.title.clone(),
        kind: "datapack".into(),
        version: version.clone(),
        file: file.clone(),
        directory: format!("saves/{world}/datapacks"),
        dependency,
    }];
    for file in version
        .files
        .iter()
        .filter(|f| {
            f.filename != result[0].file.filename
                && f.file_type.as_deref() == Some("required-resource-pack")
        })
        .cloned()
        .collect::<Vec<_>>()
    {
        resolve::validate_file(&file, "resourcepacks")?;
        let mut record = result[0].clone();
        record.file = file;
        record.directory = "resourcepacks".into();
        record.kind = "resourcepack".into();
        result.push(record);
    }
    Ok(result)
}
pub(super) fn project_plan(
    provider: &dyn ContentProvider,
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
    request: WorldProjectRequest,
) -> Result<ContentPlan, CoreError> {
    world(paths, directory, &request.world)?;
    super::modrinth::valid_id(&request.project_id)?;
    super::modrinth::valid_id(&request.version_id)?;
    let installed = install::records(paths, directory)?;
    let mut pending = VecDeque::from([(Some(request.project_id), Some(request.version_id), false)]);
    let mut chosen = std::collections::HashMap::new();
    let mut files = vec![];
    let mut visited = 0;
    let mut optional = 0;
    while let Some((project_id, version_id, dependency)) = pending.pop_front() {
        visited += 1;
        if visited > 256 || chosen.len() >= 64 {
            return Err(CoreError::DependencyConflict);
        }
        let pinned = version_id
            .as_deref()
            .map(|id| provider.version(id))
            .transpose()?;
        let id = project_id
            .or_else(|| pinned.as_ref().map(|v| v.project_id.clone()))
            .ok_or(CoreError::DependencyConflict)?;
        if pinned.as_ref().is_some_and(|v| v.project_id != id) {
            return Err(CoreError::Integrity);
        }
        if let Some(old) = chosen.get(&id) {
            if pinned.as_ref().is_some_and(|v| &v.id != old) {
                return Err(CoreError::DependencyConflict);
            }
            continue;
        }
        let project = provider.project(&id)?;
        if project.id != id {
            return Err(CoreError::Integrity);
        }
        let version = if let Some(v) = pinned {
            v
        } else if let Some(record) = installed.iter().find(|r| {
            r.project_id == id
                && (r.directory == format!("saves/{}/datapacks", request.world)
                    || r.directory == "resourcepacks")
        }) {
            provider.version(&record.version.id)?
        } else {
            let mut versions = provider.versions(&id, &instance.minecraft_version)?;
            versions.retain(|v| {
                compatible(&project, v, instance)
                    || (project.project_type == "resourcepack"
                        && resolve::compatible(&project, v, instance))
            });
            versions.sort_by(|a, b| {
                (a.version_type != "release")
                    .cmp(&(b.version_type != "release"))
                    .then_with(|| b.date_published.cmp(&a.date_published))
            });
            versions
                .into_iter()
                .next()
                .ok_or(CoreError::DependencyConflict)?
        };
        let mut selected = if compatible(&project, &version, instance) {
            selected(&project, &version, &request.world, dependency)?
        } else if dependency
            && project.project_type == "resourcepack"
            && resolve::compatible(&project, &version, instance)
        {
            resolve::selected_files(&project, &version, true)?
        } else {
            return Err(CoreError::ContentIncompatible);
        };
        if installed.iter().any(|r| {
            r.project_id == id
                && selected.iter().any(|s| s.directory == r.directory)
                && r.version.id != version.id
        }) {
            return Err(CoreError::ContentConflict);
        }
        for dep in &version.dependencies {
            match dep.dependency_type.as_str() {
                "required" => {
                    pending.push_back((dep.project_id.clone(), dep.version_id.clone(), true))
                }
                "optional" => optional += 1,
                "embedded" | "incompatible" => (),
                _ => return Err(CoreError::DependencyConflict),
            }
        }
        chosen.insert(id, version.id.clone());
        files.append(&mut selected);
    }
    let scoped: Vec<_> = files
        .iter()
        .chain(installed.iter().filter(|r| {
            !datapack_directory(&r.directory)
                || r.directory == format!("saves/{}/datapacks", request.world)
        }))
        .collect();
    for record in &scoped {
        for dep in record
            .version
            .dependencies
            .iter()
            .filter(|d| d.dependency_type == "incompatible")
        {
            if dep.project_id.is_none() && dep.version_id.is_none() {
                return Err(CoreError::DependencyConflict);
            }
            if scoped.iter().any(|other| {
                dep.project_id
                    .as_ref()
                    .is_none_or(|id| id == &other.project_id)
                    && dep
                        .version_id
                        .as_ref()
                        .is_none_or(|id| id == &other.version.id)
            }) {
                return Err(CoreError::DependencyConflict);
            }
        }
    }
    let already = install::preflight(paths, directory, &files, &installed)?;
    Ok(ContentPlan {
        token: uuid::Uuid::new_v4().to_string(),
        instance_id: instance.id.clone(),
        new_instance: None,
        total_bytes: files.iter().map(|r| r.file.size).sum(),
        files,
        optional_dependencies: optional,
        already_installed: already,
    })
}
