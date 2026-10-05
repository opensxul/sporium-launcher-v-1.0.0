use crate::{
    content::{install, model::ContentHistory},
    error::CoreError,
    game::{
        fs::{read_limited, write_atomic},
        network::{Hash, hex, verify},
    },
    instances::{
        filesystem::Paths,
        model::{Instance, Loader, now, valid_id},
    },
};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Blob {
    pub sha256: String,
    pub sha512: String,
    pub size: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub id: String,
    pub instance_id: String,
    pub minecraft: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    pub before: BTreeMap<String, Option<Blob>>,
    pub after: BTreeMap<String, Option<Blob>>,
    pub event: ContentHistory,
}

pub fn safe_path(name: &str) -> Result<(), CoreError> {
    if matches!(
        name,
        ".sporium/project.json"
            | ".sporium/content.json"
            | ".sporium/import.json"
            | ".sporium/project-source.json"
            | ".sporium/content-update-policy.json"
    ) {
        return Ok(());
    }
    crate::packs::archive::allowed_path(name)?;
    if name
        .split('/')
        .next()
        .is_some_and(|n| n.eq_ignore_ascii_case("saves"))
    {
        let (folder, _) = name.rsplit_once('/').ok_or(CoreError::UnsafePath)?;
        if !crate::content::worlds::datapack_directory(folder) {
            return Err(CoreError::UnsafePath);
        }
    }
    Ok(())
}
pub fn digest(file: &Path) -> Result<Blob, CoreError> {
    crate::instances::filesystem::no_links(file)?;
    let mut input = fs::File::open(file)?;
    let size = input.metadata()?.len();
    if size > 500_000_000 {
        return Err(CoreError::InvalidInput);
    }
    let mut sha256 = sha2::Sha256::new();
    let mut sha512 = sha2::Sha512::new();
    let mut buffer = [0u8; 65536];
    let mut count = 0u64;
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        count += read as u64;
        if count > 500_000_000 {
            return Err(CoreError::InvalidInput);
        }
        sha256.update(&buffer[..read]);
        sha512.update(&buffer[..read]);
    }
    if count != size {
        return Err(CoreError::SourceChanged);
    }
    Ok(Blob {
        sha256: hex(&sha256.finalize()),
        sha512: hex(&sha512.finalize()),
        size,
    })
}
fn valid(blob: &Blob) -> bool {
    blob.size <= 500_000_000
        && crate::packs::archive::hash_valid(&blob.sha256, 64)
        && crate::packs::archive::hash_valid(&blob.sha512, 128)
}
fn verified(file: &Path, blob: &Blob) -> Result<bool, CoreError> {
    Ok(valid(blob)
        && verify(file, &Hash::Sha256(blob.sha256.clone()), blob.size)?
        && verify(file, &Hash::Sha512(blob.sha512.clone()), blob.size)?)
}
pub fn root(directory: &Path, id: &str) -> Result<PathBuf, CoreError> {
    valid_id(id)?;
    Ok(directory.join(".sporium/project-snapshots").join(id))
}
pub fn data(directory: &Path, id: &str, blob: &Blob) -> Result<PathBuf, CoreError> {
    if !valid(blob) {
        return Err(CoreError::Integrity);
    }
    Ok(root(directory, id)?
        .join("data")
        .join(format!("{}.bin", blob.sha512)))
}
fn copy(paths: &Paths, source: &Path, target: &Path, blob: &Blob) -> Result<(), CoreError> {
    paths.checked(source)?;
    paths.checked(target)?;
    if !verified(source, blob)? {
        return Err(CoreError::SourceChanged);
    }
    if target.exists() {
        if verified(target, blob)? {
            return Ok(());
        }
        return Err(CoreError::Integrity);
    }
    paths.mkdir(target.parent().ok_or(CoreError::UnsafePath)?)?;
    let mut temp = tempfile::NamedTempFile::new_in(target.parent().ok_or(CoreError::UnsafePath)?)?;
    std::io::copy(&mut fs::File::open(source)?, &mut temp)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    if !verified(temp.path(), blob)? {
        return Err(CoreError::SourceChanged);
    }
    temp.persist_noclobber(target)
        .map_err(|e| CoreError::Io(e.error))?;
    Ok(())
}
fn map_validate(map: &BTreeMap<String, Option<Blob>>) -> Result<(), CoreError> {
    if map.len() > 10000 {
        return Err(CoreError::Integrity);
    }
    let mut names = HashSet::new();
    let mut size = 0u64;
    for (name, blob) in map {
        safe_path(name)?;
        if !names.insert(name.to_lowercase()) {
            return Err(CoreError::Integrity);
        }
        if let Some(blob) = blob {
            if !valid(blob) {
                return Err(CoreError::Integrity);
            }
            size = size.checked_add(blob.size).ok_or(CoreError::Integrity)?;
        }
    }
    if size > 2_000_000_000 {
        return Err(CoreError::InvalidInput);
    }
    crate::packs::archive::reject_file_parents(map.keys().map(String::as_str))
}
pub fn load(paths: &Paths, directory: &Path, id: &str) -> Result<Snapshot, CoreError> {
    let value: Snapshot = serde_json::from_slice(&read_limited(
        paths,
        &root(directory, id)?.join("snapshot.json"),
        32_000_000,
    )?)?;
    if value.schema_version != 1
        || value.id != id
        || directory.file_name().and_then(|s| s.to_str()) != Some(&value.instance_id)
        || value.event.id != id
    {
        return Err(CoreError::Integrity);
    }
    valid_id(&value.instance_id)?;
    map_validate(&value.before)?;
    map_validate(&value.after)?;
    if value.before.keys().ne(value.after.keys()) {
        return Err(CoreError::Integrity);
    }
    for blob in value.before.values().chain(value.after.values()).flatten() {
        if !verified(&paths.checked(&data(directory, id, blob)?)?, blob)? {
            return Err(CoreError::Integrity);
        }
    }
    Ok(value)
}
fn settings(paths: &Paths, directory: &Path) -> Result<Vec<String>, CoreError> {
    let mut pending = vec![directory.join("config")];
    let mut result = vec![];
    let mut size = 0u64;
    for name in [
        "options.txt",
        "optionsof.txt",
        "optionsshaders.txt",
        "servers.dat",
    ] {
        let p = paths.checked(&directory.join(name))?;
        if p.is_file() {
            pending.push(p);
        }
    }
    while let Some(file) = pending.pop() {
        let file = paths.checked(&file)?;
        if !file.exists() {
            continue;
        }
        if file.is_dir() {
            for entry in fs::read_dir(&file)? {
                pending.push(entry?.path());
            }
        } else {
            size += file.metadata()?.len();
            if size > 128_000_000 || file.metadata()?.len() > 32_000_000 {
                return Err(CoreError::InvalidInput);
            }
            result.push(
                file.strip_prefix(directory)
                    .map_err(|_| CoreError::UnsafePath)?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
        if pending.len() + result.len() > 4096 {
            return Err(CoreError::InvalidInput);
        }
    }
    Ok(result)
}
pub fn apply(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
    mut desired: BTreeMap<String, Option<PathBuf>>,
    action: &str,
    titles: Vec<String>,
) -> Result<String, CoreError> {
    let journal = paths.checked(&directory.join(".sporium/project-change.json"))?;
    if journal.exists() {
        return Err(CoreError::ContentConflict);
    }
    for name in settings(paths, directory)? {
        desired
            .entry(name.clone())
            .or_insert_with(|| Some(directory.join(name)));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let mut before = BTreeMap::new();
    let mut after = BTreeMap::new();
    for (name, source) in desired {
        safe_path(&name)?;
        let target = paths.checked(&directory.join(&name))?;
        let old = if target.exists() {
            let blob = digest(&target)?;
            copy(paths, &target, &data(directory, &id, &blob)?, &blob)?;
            Some(blob)
        } else {
            None
        };
        let next = if let Some(source) = source {
            let source = paths.checked(&source)?;
            let blob = digest(&source)?;
            copy(paths, &source, &data(directory, &id, &blob)?, &blob)?;
            Some(blob)
        } else {
            None
        };
        before.insert(name.clone(), old);
        after.insert(name, next);
    }
    map_validate(&before)?;
    map_validate(&after)?;
    let value = Snapshot {
        schema_version: 1,
        id: id.clone(),
        instance_id: instance.id.clone(),
        minecraft: instance.minecraft_version.clone(),
        loader: instance.loader,
        loader_version: instance.loader_version.clone(),
        before,
        after,
        event: ContentHistory {
            id: id.clone(),
            timestamp: now(),
            action: action.into(),
            titles,
        },
    };
    write_atomic(
        paths,
        &root(directory, &id)?.join("snapshot.json"),
        &serde_json::to_vec(&value)?,
    )?;
    // Check the complete source state again before publishing the recoverable journal.
    preflight(paths, directory, &value, false)?;
    write_atomic(
        paths,
        &journal,
        &serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"id":id}))?,
    )?;
    recover(paths, directory)?;
    Ok(id)
}
fn matches(
    paths: &Paths,
    directory: &Path,
    name: &str,
    blob: &Option<Blob>,
) -> Result<bool, CoreError> {
    let file = paths.checked(&directory.join(name))?;
    match blob {
        Some(blob) => verified(&file, blob),
        None => Ok(!file.exists()),
    }
}
fn preflight(
    paths: &Paths,
    directory: &Path,
    value: &Snapshot,
    recovery: bool,
) -> Result<(), CoreError> {
    for (name, before) in &value.before {
        if name.starts_with("saves/") {
            crate::content::worlds::validate_destination(
                paths,
                directory,
                name.rsplit_once('/').ok_or(CoreError::UnsafePath)?.0,
            )?;
        }
        if !matches(paths, directory, name, before)?
            && !(recovery && matches(paths, directory, name, &value.after[name])?)
            && !(recovery && !paths.checked(&directory.join(name))?.exists())
        {
            return Err(CoreError::ContentConflict);
        }
    }
    Ok(())
}
pub fn recover(paths: &Paths, directory: &Path) -> Result<(), CoreError> {
    let file = paths.checked(&directory.join(".sporium/project-change.json"))?;
    if !file.exists() {
        return Ok(());
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Journal {
        schema_version: u32,
        id: String,
    }
    let journal: Journal = serde_json::from_slice(&read_limited(paths, &file, 1024)?)?;
    if journal.schema_version != 1 {
        return Err(CoreError::SchemaTooNew);
    }
    let value = load(paths, directory, &journal.id)?;
    preflight(paths, directory, &value, true)?;
    for (name, after) in &value.after {
        if matches(paths, directory, name, after)? {
            continue;
        }
        let target = paths.checked(&directory.join(name))?;
        if target.exists() {
            paths.remove(&target)?;
        }
        if let Some(blob) = after {
            copy(paths, &data(directory, &journal.id, blob)?, &target, blob)?;
        }
    }
    crate::content::append_project_history(
        paths,
        directory,
        ContentHistory {
            id: format!("snapshot:{}", value.id),
            timestamp: value.event.timestamp,
            action: "snapshot".into(),
            titles: vec!["Project restore point".into()],
        },
    )?;
    crate::content::append_project_history(paths, directory, value.event)?;
    paths.remove(&file)
}
pub fn restore(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
    id: &str,
    include_settings: bool,
) -> Result<String, CoreError> {
    let saved = load(paths, directory, id)?;
    if saved.minecraft != instance.minecraft_version
        || saved.loader != instance.loader
        || saved.loader_version != instance.loader_version
    {
        return Err(CoreError::ContentIncompatible);
    }
    let mut desired = BTreeMap::new();
    let locked_settings: HashSet<String> = if let Some(Some(blob)) =
        saved.before.get(".sporium/project.json")
    {
        let bytes = read_limited(paths, &data(directory, id, blob)?, 8_000_000)?;
        let manifest: super::model::ProjectManifest = serde_json::from_slice(&bytes)?;
        super::validate(&manifest)?;
        manifest
            .files
            .into_iter()
            .filter(|file| {
                file.policy == super::model::FilePolicy::RequiredLocked && is_setting(&file.path)
            })
            .map(|file| file.path)
            .collect()
    } else {
        HashSet::new()
    };
    for name in [
        ".sporium/project.json",
        ".sporium/import.json",
        ".sporium/project-source.json",
    ] {
        if let Some(after) = saved.after.get(name)
            && !matches(paths, directory, name, after)?
        {
            return Err(CoreError::RecordConflict);
        }
    }
    for (name, blob) in &saved.before {
        if !include_settings && is_setting(name) && !locked_settings.contains(name) {
            continue;
        }
        if !is_setting(name) && !name.starts_with(".sporium/") {
            let target = paths.checked(&directory.join(name))?;
            if target.exists() && !matches(paths, directory, name, &saved.after[name])? {
                return Err(CoreError::ContentConflict);
            }
        }
        desired.insert(
            name.clone(),
            blob.as_ref().map(|b| data(directory, id, b)).transpose()?,
        );
    }
    // Retain receipts for user content added outside this transaction's managed paths.
    if let Some(source) = desired.get(".sporium/content.json") {
        let mut old: Vec<crate::content::model::ContentRecord> = if let Some(source) = source {
            let old: serde_json::Value =
                serde_json::from_slice(&read_limited(paths, source, 16_000_000)?)?;
            serde_json::from_value(old.get("files").cloned().ok_or(CoreError::Integrity)?)?
        } else {
            vec![]
        };
        for record in install::records(paths, directory)? {
            let name = format!("{}/{}", record.directory, record.file.filename);
            if !saved.before.contains_key(&name)
                && !old.iter().any(|r| {
                    r.directory == record.directory
                        && r.file.filename.eq_ignore_ascii_case(&record.file.filename)
                })
            {
                old.push(record);
            }
        }
        let stage = paths.checked(&directory.join(".sporium/project-restore-stage"))?;
        paths.mkdir(&stage)?;
        install::write_records(paths, &stage, &old)?;
        desired.insert(
            ".sporium/content.json".into(),
            Some(stage.join(".sporium/content.json")),
        );
    }
    apply(
        paths,
        directory,
        instance,
        desired,
        "restore",
        vec![format!("{}: {}", instance.name, id)],
    )
}
pub fn is_setting(name: &str) -> bool {
    name.starts_with("config/")
        || matches!(
            name,
            "options.txt" | "optionsof.txt" | "optionsshaders.txt" | "servers.dat"
        )
}
