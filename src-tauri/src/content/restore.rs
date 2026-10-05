use super::{install, manage, model::*, update};
use crate::{
    error::CoreError,
    game::fs::{read_limited, write_atomic},
    instances::{
        filesystem::Paths,
        model::{Instance, now, valid_id},
    },
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    token: String,
    source: String,
    before: Vec<ContentRecord>,
    after: Vec<ContentRecord>,
    event: ContentHistory,
}

fn same(a: &[ContentRecord], b: &[ContentRecord]) -> Result<bool, CoreError> {
    Ok(serde_json::to_vec(a)? == serde_json::to_vec(b)?)
}

fn snapshot(paths: &Paths, directory: &Path, id: &str) -> Result<update::Snapshot, CoreError> {
    valid_id(id)?;
    let file = directory
        .join(".sporium/content-snapshots")
        .join(id)
        .join("snapshot.json");
    let value: update::Snapshot = serde_json::from_slice(&read_limited(paths, &file, 32_000_000)?)?;
    if value.schema != 1
        || directory.file_name().and_then(|s| s.to_str()) != Some(&value.instance.id)
    {
        return Err(CoreError::Integrity);
    }
    value.instance.validate()?;
    install::validate_records(&value.files)?;
    for (index, record) in value.files.iter().enumerate() {
        if !install::verified(
            &update::snapshot_file(paths, directory, id, index)?,
            &record.file,
        )? {
            return Err(CoreError::Integrity);
        }
    }
    Ok(value)
}

pub(super) fn points(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
) -> Result<Vec<ContentRestorePoint>, CoreError> {
    let history = manage::history(paths, directory)?;
    let root = paths.checked(&directory.join(".sporium/content-snapshots"))?;
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut result = vec![];
    for entry in fs::read_dir(root)? {
        if result.len() >= 4096 {
            return Err(CoreError::Integrity);
        }
        let entry = entry?;
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if valid_id(&id).is_err() {
            continue;
        }
        let value = snapshot(paths, directory, &id);
        let event = history.iter().find(|e| e.id == format!("snapshot:{id}"));
        result.push(ContentRestorePoint {
            id: id.clone(),
            timestamp: event.map_or(0, |e| e.timestamp),
            title: value
                .as_ref()
                .map_or_else(|_| instance.name.clone(), |s| s.instance.name.clone()),
            files: value.as_ref().map_or(0, |s| s.files.len() as u32),
            available: value
                .as_ref()
                .is_ok_and(|s| compatible(&s.instance, instance)),
            settings_available: value.as_ref().is_ok_and(|s| {
                !s.settings.is_empty() && settings_valid(paths, directory, &id, s).is_ok()
            }),
            project_point: false,
        });
    }
    result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| a.id.cmp(&b.id)));
    Ok(result)
}

fn compatible(a: &Instance, b: &Instance) -> bool {
    a.id == b.id
        && a.minecraft_version == b.minecraft_version
        && a.loader == b.loader
        && a.loader_version == b.loader_version
}
fn settings_valid(
    paths: &Paths,
    directory: &Path,
    id: &str,
    saved: &update::Snapshot,
) -> Result<(), CoreError> {
    if saved.settings.len() != saved.settings_hashes.len() {
        return Err(CoreError::Integrity);
    }
    for name in &saved.settings {
        crate::projects::transaction::safe_path(name)?;
        if !crate::projects::transaction::is_setting(name) {
            return Err(CoreError::UnsafePath);
        }
        let file = paths.checked(
            &directory
                .join(".sporium/content-snapshots")
                .join(id)
                .join("settings")
                .join(name),
        )?;
        if saved.settings_hashes.get(name) != Some(&crate::projects::transaction::digest(&file)?) {
            return Err(CoreError::Integrity);
        }
    }
    Ok(())
}
pub(super) fn restore_with_settings(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
    source: &str,
) -> Result<(), CoreError> {
    let saved = snapshot(paths, directory, source)?;
    if !compatible(&saved.instance, instance) {
        return Err(CoreError::ContentIncompatible);
    }
    settings_valid(paths, directory, source, &saved)?;
    let before = install::records(paths, directory)?;
    preflight(paths, directory, &before, &saved.files, false)?;
    crate::projects::guard_restore(paths, directory, &saved.files)?;
    let mut desired = std::collections::BTreeMap::new();
    for record in &before {
        desired.insert(
            format!("{}/{}", record.directory, record.file.filename),
            None,
        );
    }
    for (index, record) in saved.files.iter().enumerate() {
        desired.insert(
            format!("{}/{}", record.directory, record.file.filename),
            Some(update::snapshot_file(paths, directory, source, index)?),
        );
    }
    for name in &saved.settings {
        desired.insert(
            name.clone(),
            Some(
                directory
                    .join(".sporium/content-snapshots")
                    .join(source)
                    .join("settings")
                    .join(name),
            ),
        );
    }
    let stage = paths.checked(&directory.join(".sporium/restore-settings-stage"))?;
    paths.mkdir(&stage)?;
    install::write_records(paths, &stage, &saved.files)?;
    desired.insert(
        ".sporium/content.json".into(),
        Some(stage.join(".sporium/content.json")),
    );
    crate::projects::transaction::apply(
        paths,
        directory,
        instance,
        desired,
        "restore",
        vec![instance.name.clone()],
    )?;
    Ok(())
}

// Only receipt-owned files participate. Unknown files, settings and worlds are retained.
fn preflight(
    paths: &Paths,
    directory: &Path,
    before: &[ContentRecord],
    after: &[ContentRecord],
    recovering: bool,
) -> Result<(), CoreError> {
    for old in before {
        let target = install::target(paths, directory, old)?;
        if (!recovering || target.exists())
            && !install::verified(&target, &old.file)?
            && !(recovering
                && after.iter().any(|r| {
                    r.directory == old.directory
                        && r.file.filename.eq_ignore_ascii_case(&old.file.filename)
                        && install::verified(&target, &r.file).unwrap_or(false)
                }))
        {
            return Err(CoreError::ContentConflict);
        }
    }
    for record in after {
        super::worlds::validate_destination(paths, directory, &record.directory)?;
        let target = install::target(paths, directory, record)?;
        if !recovering
            && target.exists()
            && !before.iter().any(|r| {
                r.directory == record.directory
                    && r.file.filename.eq_ignore_ascii_case(&record.file.filename)
            })
        {
            return Err(CoreError::ContentConflict);
        }
        if target.exists()
            && !install::verified(&target, &record.file)?
            && !before.iter().any(|r| {
                r.directory == record.directory
                    && r.file.filename.eq_ignore_ascii_case(&record.file.filename)
                    && install::verified(&target, &r.file).unwrap_or(false)
            })
        {
            return Err(CoreError::ContentConflict);
        }
        if matches!(record.directory.as_str(), "mods" | "mods_disabled") {
            let other = paths.checked(
                &directory
                    .join(if record.directory == "mods" {
                        "mods_disabled"
                    } else {
                        "mods"
                    })
                    .join(&record.file.filename),
            )?;
            if other.exists()
                && !before.iter().any(|r| {
                    install::target(paths, directory, r).is_ok_and(|p| p == other)
                        && install::verified(&other, &r.file).unwrap_or(false)
                })
            {
                return Err(CoreError::ContentConflict);
            }
        }
    }
    Ok(())
}

pub(super) fn restore(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
    source: &str,
) -> Result<(), CoreError> {
    let saved = snapshot(paths, directory, source)?;
    crate::projects::guard_restore(paths, directory, &saved.files)?;
    if !compatible(&saved.instance, instance) {
        return Err(CoreError::ContentIncompatible);
    }
    let before = install::records(paths, directory)?;
    preflight(paths, directory, &before, &saved.files, false)?;
    let token = uuid::Uuid::new_v4().to_string();
    let root = paths.checked(&directory.join(".sporium/content-snapshots").join(&token))?;
    for (index, record) in before.iter().enumerate() {
        update::copy_verified(
            paths,
            &install::target(paths, directory, record)?,
            &update::snapshot_file(paths, directory, &token, index)?,
            &record.file,
        )?;
    }
    write_atomic(
        paths,
        &root.join("snapshot.json"),
        &serde_json::to_vec(&update::Snapshot {
            schema: 1,
            instance: instance.clone(),
            files: before.clone(),
            settings: vec![],
            settings_hashes: Default::default(),
        })?,
    )?;
    let journal = Journal {
        schema: 1,
        token: token.clone(),
        source: source.into(),
        before,
        after: saved.files,
        event: ContentHistory {
            id: token,
            timestamp: now(),
            action: "restore".into(),
            titles: vec![instance.name.clone()],
        },
    };
    let file = paths.checked(&directory.join(".sporium/content-restore.json"))?;
    if file.exists() {
        return Err(CoreError::ContentConflict);
    }
    write_atomic(paths, &file, &serde_json::to_vec(&journal)?)?;
    recover(paths, directory)
}

pub(super) fn recover(paths: &Paths, directory: &Path) -> Result<(), CoreError> {
    let file = paths.checked(&directory.join(".sporium/content-restore.json"))?;
    if !file.exists() {
        return Ok(());
    }
    let journal: Journal = serde_json::from_slice(&read_limited(paths, &file, 64_000_000)?)?;
    if journal.schema != 1
        || journal.event.id != journal.token
        || journal.event.action != "restore"
        || journal.token == journal.source
    {
        return Err(CoreError::Integrity);
    }
    let undo = snapshot(paths, directory, &journal.token)?;
    let saved = snapshot(paths, directory, &journal.source)?;
    if !compatible(&undo.instance, &saved.instance)
        || !same(&undo.files, &journal.before)?
        || !same(&saved.files, &journal.after)?
    {
        return Err(CoreError::Integrity);
    }
    let current = install::records(paths, directory)?;
    if !same(&current, &journal.before)? && !same(&current, &journal.after)? {
        return Err(CoreError::RecordConflict);
    }
    preflight(paths, directory, &journal.before, &journal.after, true)?;
    for old in &journal.before {
        let target = install::target(paths, directory, old)?;
        if target.exists()
            && !journal.after.iter().any(|r| {
                r.directory == old.directory
                    && r.file.filename.eq_ignore_ascii_case(&old.file.filename)
                    && install::verified(&target, &r.file).unwrap_or(false)
            })
        {
            if !install::verified(&target, &old.file)? {
                return Err(CoreError::ContentConflict);
            }
            paths.remove(&target)?;
        }
    }
    for (index, record) in journal.after.iter().enumerate() {
        let target = install::target(paths, directory, record)?;
        if !target.exists() {
            update::copy_verified(
                paths,
                &update::snapshot_file(paths, directory, &journal.source, index)?,
                &target,
                &record.file,
            )?;
        }
    }
    install::write_records(paths, directory, &journal.after)?;
    manage::append_history(
        paths,
        directory,
        ContentHistory {
            id: format!("snapshot:{}", journal.token),
            timestamp: journal.event.timestamp,
            action: "snapshot".into(),
            titles: vec![undo.instance.name],
        },
    )?;
    manage::append_history(paths, directory, journal.event)?;
    paths.remove(&file)
}
