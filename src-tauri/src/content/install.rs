use super::{model::*, resolve::validate_file};
use crate::{
    error::CoreError,
    game::{
        fs::{read_limited, write_atomic},
        network::{Hash, verify},
    },
    instances::filesystem::Paths,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    files: Vec<ContentRecord>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    schema: u32,
    token: String,
    files: Vec<ContentRecord>,
}
pub fn records(paths: &Paths, directory: &Path) -> Result<Vec<ContentRecord>, CoreError> {
    let path = paths.checked(&directory.join(".sporium/content.json"))?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let receipt: Receipt = serde_json::from_slice(&read_limited(paths, &path, 16_000_000)?)?;
    if !matches!(receipt.schema, 1 | 2) {
        return Err(CoreError::SchemaTooNew);
    }
    validate_records(&receipt.files)?;
    Ok(receipt.files)
}
pub(super) fn validate_records(files: &[ContentRecord]) -> Result<(), CoreError> {
    if files.len() > 4096 {
        return Err(CoreError::Integrity);
    }
    let mut targets = std::collections::HashSet::new();
    for record in files {
        super::modrinth::valid_id(&record.project_id)?;
        super::modrinth::valid_id(&record.version.id)?;
        validate_record(record)?;
        if record.project_id != record.version.project_id
            || !targets.insert(
                format!(
                    "{}/{}",
                    if record.directory == "mods_disabled" {
                        "mods"
                    } else {
                        &record.directory
                    },
                    record.file.filename
                )
                .to_lowercase(),
            )
        {
            return Err(CoreError::Integrity);
        }
    }
    Ok(())
}
fn validate_record(record: &ContentRecord) -> Result<(), CoreError> {
    match record.provider.as_str() {
        "modrinth" if record.local.is_none() => validate_file(&record.file, &record.directory),
        "local"
            if record.local.is_some()
                && record.file.url.is_empty()
                && (matches!(
                    record.directory.as_str(),
                    "mods" | "mods_disabled" | "resourcepacks" | "shaderpacks"
                ) || super::worlds::datapack_directory(&record.directory)) =>
        {
            crate::instances::model::valid_id(&record.project_id)?;
            super::resolve::validate_payload(&record.file, &record.directory)
        }
        _ => Err(CoreError::Integrity),
    }
}
pub(crate) fn write_records(
    paths: &Paths,
    directory: &Path,
    files: &[ContentRecord],
) -> Result<(), CoreError> {
    validate_records(files)?;
    write_atomic(
        paths,
        &directory.join(".sporium/content.json"),
        &serde_json::to_vec(&Receipt {
            schema: if files.iter().any(|r| r.provider == "local") {
                2
            } else {
                1
            },
            files: files.to_vec(),
        })?,
    )
}
pub fn target(
    paths: &Paths,
    directory: &Path,
    record: &ContentRecord,
) -> Result<PathBuf, CoreError> {
    validate_record(record)?;
    paths.checked(
        &directory
            .join(&record.directory)
            .join(&record.file.filename),
    )
}
pub fn verified(path: &Path, file: &ContentFile) -> Result<bool, CoreError> {
    Ok(
        verify(path, &Hash::Sha512(file.hashes.sha512.clone()), file.size)?
            && verify(path, &Hash::Sha1(file.hashes.sha1.clone()), file.size)?,
    )
}
pub fn preflight(
    paths: &Paths,
    directory: &Path,
    files: &[ContentRecord],
    installed: &[ContentRecord],
) -> Result<u32, CoreError> {
    let mut present = 0;
    for record in files {
        let current = target(paths, directory, record)?;
        if !current.exists() || !verified(&current, &record.file)? {
            crate::projects::guard_user_file(
                paths,
                directory,
                &format!("{}/{}", record.directory, record.file.filename),
                true,
            )?;
        }
        super::worlds::validate_destination(paths, directory, &record.directory)?;
        let target = target(paths, directory, record)?;
        if target.exists() {
            let known = installed.iter().any(|r| {
                r.project_id == record.project_id
                    && r.version.id == record.version.id
                    && r.directory == record.directory
                    && r.file.filename.eq_ignore_ascii_case(&record.file.filename)
                    && r.file.hashes.sha512 == record.file.hashes.sha512
            });
            if !known || !verified(&target, &record.file)? {
                return Err(CoreError::ContentConflict);
            }
            present += 1;
        }
        if record.directory == "mods" {
            let disabled =
                paths.checked(&directory.join("mods_disabled").join(&record.file.filename))?;
            if disabled.exists() {
                return Err(CoreError::ContentConflict);
            }
        }
    }
    Ok(present)
}
pub fn stage(
    paths: &Paths,
    directory: &Path,
    token: &str,
    index: usize,
) -> Result<PathBuf, CoreError> {
    crate::instances::model::valid_id(token)?;
    paths.checked(
        &directory
            .join(".sporium/content-staging")
            .join(token)
            .join(format!("{index}.bin")),
    )
}
pub fn commit(paths: &Paths, directory: &Path, plan: &ContentPlan) -> Result<(), CoreError> {
    let installed = records(paths, directory)?;
    preflight(paths, directory, &plan.files, &installed)?;
    let pending = Pending {
        schema: 1,
        token: plan.token.clone(),
        files: plan.files.clone(),
    };
    let journal = directory.join(".sporium/content-pending.json");
    if paths.checked(&journal)?.exists() {
        return Err(CoreError::ContentConflict);
    }
    write_atomic(paths, &journal, &serde_json::to_vec(&pending)?)?;
    recover(paths, directory)
}
pub(crate) fn recover(paths: &Paths, directory: &Path) -> Result<(), CoreError> {
    crate::projects::transaction::recover(paths, directory)?;
    super::worlds::recover(paths, directory)?;
    super::adopt::recover(paths, directory)?;
    super::update::recover(paths, directory)?;
    super::restore::recover(paths, directory)?;
    super::manage::recover(paths, directory)?;
    let journal = paths.checked(&directory.join(".sporium/content-pending.json"))?;
    if !journal.exists() {
        return Ok(());
    }
    let pending: Pending = serde_json::from_slice(&read_limited(paths, &journal, 16_000_000)?)?;
    if pending.schema != 1 {
        return Err(CoreError::SchemaTooNew);
    }
    validate_records(&pending.files)?;
    crate::instances::model::valid_id(&pending.token)?;
    let mut installed = records(paths, directory)?;
    // Validate every staged artifact before publishing any. Recovery never needs the network.
    for (index, record) in pending.files.iter().enumerate() {
        super::worlds::validate_destination(paths, directory, &record.directory)?;
        let staged = stage(paths, directory, &pending.token, index)?;
        if !verified(&staged, &record.file)? {
            return Err(CoreError::Integrity);
        }
        let target = target(paths, directory, record)?;
        if target.exists() && !verified(&target, &record.file)? {
            return Err(CoreError::ContentConflict);
        }
    }
    for (index, record) in pending.files.iter().enumerate() {
        let target = target(paths, directory, record)?;
        if !target.exists() {
            paths.mkdir(target.parent().ok_or(CoreError::UnsafePath)?)?;
            let mut temp =
                tempfile::NamedTempFile::new_in(target.parent().ok_or(CoreError::UnsafePath)?)?;
            std::io::copy(
                &mut fs::File::open(stage(paths, directory, &pending.token, index)?)?,
                &mut temp,
            )?;
            temp.flush()?;
            temp.as_file().sync_all()?;
            paths.checked(&target)?;
            temp.persist_noclobber(&target)
                .map_err(|e| CoreError::Io(e.error))?;
        }
        installed.retain(|r| {
            !(r.directory == record.directory
                && r.file.filename.eq_ignore_ascii_case(&record.file.filename))
        });
        installed.push(record.clone());
    }
    validate_records(&installed)?;
    write_records(paths, directory, &installed)?;
    super::manage::append_history(
        paths,
        directory,
        ContentHistory {
            id: pending.token.clone(),
            timestamp: crate::instances::model::now(),
            action: "add".into(),
            titles: pending.files.iter().map(|r| r.title.clone()).collect(),
        },
    )?;
    paths.remove(&journal)?;
    paths.remove(
        &directory
            .join(".sporium/content-staging")
            .join(&pending.token),
    )?;
    Ok(())
}
