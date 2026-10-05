//! Explicit receipt adoption: existing user files are never moved or rewritten.
use super::{install, local, manage, model::*, provider::ContentProvider, resolve};
use crate::{
    error::CoreError,
    game::fs::{read_limited, write_atomic},
    instances::{filesystem::Paths, model::Instance},
};
use serde::{Deserialize, Serialize};
use sha1::Digest;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub(super) fn source(
    paths: &Paths,
    directory: &Path,
    folder: &str,
    filename: &str,
) -> Result<PathBuf, CoreError> {
    resolve::validate_payload(
        &ContentFile {
            id: None,
            filename: filename.into(),
            url: String::new(),
            hashes: ContentHashes {
                sha1: "0".repeat(40),
                sha512: "0".repeat(128),
            },
            size: 1,
            primary: true,
            file_type: None,
        },
        folder,
    )?;
    let path = paths.checked(&directory.join(folder).join(filename))?;
    if !path.is_file() {
        return Err(CoreError::ContentUnsupported);
    }
    let size = std::fs::metadata(&path)?.len();
    if size == 0 || size > 200_000_000 {
        return Err(CoreError::Integrity);
    }
    Ok(path)
}

fn artifact(path: &Path, filename: &str) -> Result<ContentFile, CoreError> {
    let input = File::open(path)?;
    let size = input.metadata()?.len();
    if size == 0 || size > 200_000_000 {
        return Err(CoreError::Integrity);
    }
    let mut input = input.take(200_000_001);
    let mut sha1 = sha1::Sha1::new();
    let mut sha512 = sha2::Sha512::new();
    let mut buffer = [0; 65536];
    let mut bytes = 0;
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        sha1.update(&buffer[..n]);
        sha512.update(&buffer[..n]);
    }
    if bytes != size {
        return Err(CoreError::SourceChanged);
    }
    Ok(ContentFile {
        id: None,
        filename: filename.into(),
        url: String::new(),
        hashes: ContentHashes {
            sha1: crate::game::network::hex(&sha1.finalize()),
            sha512: crate::game::network::hex(&sha512.finalize()),
        },
        size,
        primary: true,
        file_type: None,
    })
}

pub(super) fn prepare(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
    request: &ContentAdoptionRequest,
    provider: &dyn ContentProvider,
) -> Result<ContentAdoptionPlan, CoreError> {
    let path = source(paths, directory, &request.directory, &request.filename)?;
    if crate::game::skinmod::owned(&path)? {
        return Err(CoreError::ContentUnsupported);
    }
    let kind = match request.directory.as_str() {
        "mods" | "mods_disabled" => "mod",
        "resourcepacks" => "resourcepack",
        "shaderpacks" => "shader",
        _ => return Err(CoreError::ContentUnsupported),
    };
    let (title, version_number, metadata) = if kind == "mod" {
        local::inspect(&path, instance)?
    } else {
        let zip = zip::ZipArchive::new(File::open(&path)?).map_err(|_| CoreError::Integrity)?;
        if zip.len() > 100_000 {
            return Err(CoreError::Integrity);
        }
        (
            request.filename.clone(),
            "?".into(),
            LocalMetadata {
                warnings: vec!["unknown_metadata".into(), "unknown_version".into()],
                ..Default::default()
            },
        )
    };
    let file = artifact(&path, &request.filename)?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut warnings = metadata.warnings.clone();
    let mut record = ContentRecord {
        local: Some(metadata.clone()),
        icon_url: None,
        provider: "local".into(),
        project_id: id.clone(),
        title,
        kind: kind.into(),
        version: ContentVersion {
            id: id.clone(),
            project_id: id,
            name: request.filename.clone(),
            version_number,
            version_type: "local".into(),
            date_published: String::new(),
            status: "local".into(),
            game_versions: vec![],
            loaders: vec![metadata.loader.clone()],
            environment: "unknown".into(),
            dependencies: vec![],
            files: vec![file.clone()],
        },
        file: file.clone(),
        directory: request.directory.clone(),
        dependency: false,
    };
    if request.recognize {
        match provider.version_from_hash(&file.hashes.sha512) {
            Ok(Some(version)) => {
                super::modrinth::valid_id(&version.id)?;
                super::modrinth::valid_id(&version.project_id)?;
                let found = version
                    .files
                    .iter()
                    .find(|f| {
                        f.size == file.size
                            && f.hashes.sha512.eq_ignore_ascii_case(&file.hashes.sha512)
                            && f.hashes.sha1.eq_ignore_ascii_case(&file.hashes.sha1)
                    })
                    .ok_or(CoreError::Integrity)?;
                resolve::validate_file(found, &request.directory)?;
                if matches!(
                    found.file_type.as_deref(),
                    Some("sources-jar" | "dev-jar" | "javadoc-jar" | "signature")
                ) {
                    return Err(CoreError::ContentUnsupported);
                }
                match provider.project(&version.project_id) {
                    Ok(project) => {
                        if project.project_type != kind
                            || !resolve::compatible(&project, &version, instance)
                        {
                            return Err(CoreError::ContentIncompatible);
                        }
                        let mut found = found.clone();
                        found.filename.clone_from(&request.filename);
                        record.file = found;
                        record.project_id = project.id;
                        record.title = project.title;
                        record.icon_url = project.icon_url;
                        record.version = version;
                        record.provider = "modrinth".into();
                        record.local = None;
                    }
                    Err(CoreError::Network | CoreError::RateLimited) => {
                        warnings.push("match_unavailable".into())
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok(None) => warnings.push("match_not_found".into()),
            Err(CoreError::Network | CoreError::RateLimited) => {
                warnings.push("match_unavailable".into())
            }
            Err(error) => return Err(error),
        }
    }
    preflight(paths, directory, instance, &record)?;
    if !install::verified(&path, &record.file)? {
        return Err(CoreError::SourceChanged);
    }
    warnings.sort();
    warnings.dedup();
    let diagnostics = if kind == "mod" {
        super::diagnostics::scan(paths, directory, instance, &[])?
    } else {
        ModDiagnostics {
            complete: true,
            errors: 0,
            warnings: 0,
            mods: vec![],
        }
    };
    if super::diagnostics::selected_issues(&diagnostics, std::slice::from_ref(&record)) {
        warnings.push("dependency_issues".into());
    }
    Ok(ContentAdoptionPlan {
        diagnostics,
        metadata,
        plan: ContentPlan {
            new_instance: None,
            token: uuid::Uuid::new_v4().to_string(),
            instance_id: instance.id.clone(),
            files: vec![record],
            optional_dependencies: 0,
            already_installed: 0,
            total_bytes: 0,
        },
        warnings,
    })
}

pub(super) fn preflight(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
    record: &ContentRecord,
) -> Result<(), CoreError> {
    manage::history(paths, directory)?;
    let before = install::records(paths, directory)?;
    if before.iter().any(|r| r.project_id == record.project_id) {
        return Err(CoreError::ContentConflict);
    }
    let mut after = before;
    after.push(record.clone());
    install::validate_records(&after)?;
    if serde_json::to_vec(&after)?.len() > 15_000_000 {
        return Err(CoreError::Integrity);
    }
    if record.kind == "mod" {
        let selected = source(paths, directory, &record.directory, &record.file.filename)?;
        let (_, _, meta) = local::inspect(&selected, instance)?;
        for folder in ["mods", "mods_disabled"] {
            let root = paths.checked(&directory.join(folder))?;
            if !root.exists() {
                continue;
            }
            let mut count = 0;
            for entry in std::fs::read_dir(root)? {
                count += 1;
                if count > 4096 {
                    return Err(CoreError::Integrity);
                }
                let path = paths.checked(&entry?.path())?;
                if path == selected {
                    continue;
                }
                if path
                    .file_name()
                    .is_some_and(|s| s.eq_ignore_ascii_case(&record.file.filename))
                {
                    return Err(CoreError::ContentConflict);
                }
                if path.is_file()
                    && path
                        .extension()
                        .is_some_and(|s| s.eq_ignore_ascii_case("jar"))
                    && let Ok((_, _, other)) = local::inspect(&path, instance)
                    && meta.mod_ids.iter().any(|id| other.mod_ids.contains(id))
                {
                    return Err(CoreError::ContentConflict);
                }
            }
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    before: Vec<ContentRecord>,
    added: ContentRecord,
    event: ContentHistory,
}
pub(super) fn commit(
    paths: &Paths,
    directory: &Path,
    record: &ContentRecord,
) -> Result<(), CoreError> {
    let journal = Journal {
        schema: 1,
        before: install::records(paths, directory)?,
        added: record.clone(),
        event: ContentHistory {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: crate::instances::model::now(),
            action: "adopt".into(),
            titles: vec![record.title.clone()],
        },
    };
    write_atomic(
        paths,
        &directory.join(".sporium/content-adoption.json"),
        &serde_json::to_vec(&journal)?,
    )?;
    recover(paths, directory)
}
pub(super) fn recover(paths: &Paths, directory: &Path) -> Result<(), CoreError> {
    let path = paths.checked(&directory.join(".sporium/content-adoption.json"))?;
    if !path.exists() {
        return Ok(());
    }
    let journal: Journal = serde_json::from_slice(&read_limited(paths, &path, 32_000_000)?)?;
    if journal.schema != 1 {
        return Err(CoreError::SchemaTooNew);
    }
    crate::instances::model::valid_id(&journal.event.id)?;
    if journal.event.action != "adopt" || journal.event.titles != vec![journal.added.title.clone()]
    {
        return Err(CoreError::Integrity);
    }
    install::validate_records(&journal.before)?;
    let mut after = journal.before.clone();
    after.push(journal.added.clone());
    install::validate_records(&after)?;
    if serde_json::to_vec(&after)?.len() > 15_000_000 {
        return Err(CoreError::Integrity);
    }
    let current = serde_json::to_vec(&install::records(paths, directory)?)?;
    if current != serde_json::to_vec(&journal.before)? && current != serde_json::to_vec(&after)? {
        return Err(CoreError::RecordConflict);
    }
    let file = install::target(paths, directory, &journal.added)?;
    if crate::game::skinmod::owned(&file)? || !install::verified(&file, &journal.added.file)? {
        return Err(CoreError::SourceChanged);
    }
    install::write_records(paths, directory, &after)?;
    manage::append_history(paths, directory, journal.event)?;
    paths.remove(&path)
}
