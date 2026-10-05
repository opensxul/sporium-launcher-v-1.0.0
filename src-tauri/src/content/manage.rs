use super::{install, model::*};
use crate::{
    error::CoreError,
    game::fs::{read_limited, write_atomic},
    instances::{filesystem::Paths, model::now},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, io::Write, path::Path};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryFile {
    schema: u32,
    entries: Vec<ContentHistory>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangeJournal {
    schema: u32,
    token: String,
    action: ContentAction,
    before: Vec<ContentRecord>,
    after: Vec<ContentRecord>,
    selected: Vec<ContentRecord>,
    event: ContentHistory,
}

pub fn history(paths: &Paths, directory: &Path) -> Result<Vec<ContentHistory>, CoreError> {
    let file = paths.checked(&directory.join(".sporium/content-history.json"))?;
    if !file.exists() {
        return Ok(vec![]);
    }
    let file: HistoryFile = serde_json::from_slice(&read_limited(paths, &file, 8_000_000)?)?;
    if file.schema != 1 {
        return Err(CoreError::SchemaTooNew);
    }
    Ok(file.entries)
}

pub fn append_history(
    paths: &Paths,
    directory: &Path,
    mut event: ContentHistory,
) -> Result<(), CoreError> {
    let mut entries = history(paths, directory)?;
    if entries.iter().any(|e| e.id == event.id) {
        return Ok(());
    }
    event.titles = event
        .titles
        .into_iter()
        .map(|title| title.chars().take(160).collect())
        .collect();
    entries.push(event);
    if entries.len() > 500 {
        entries.drain(..entries.len() - 500);
    }
    let mut file = HistoryFile { schema: 1, entries };
    loop {
        let bytes = serde_json::to_vec(&file)?;
        if bytes.len() <= 8_000_000 {
            return write_atomic(
                paths,
                &directory.join(".sporium/content-history.json"),
                &bytes,
            );
        }
        if file.entries.len() <= 1 {
            return Err(CoreError::Integrity);
        }
        file.entries.remove(0);
    }
}

pub fn change(paths: &Paths, directory: &Path, request: ContentChange) -> Result<(), CoreError> {
    if request.files.is_empty() || request.files.len() > 4096 {
        return Err(CoreError::InvalidInput);
    }
    let before = install::records(paths, directory)?;
    history(paths, directory)?;
    let mut selected = vec![];
    let mut seen = HashSet::new();
    for item in &request.files {
        crate::projects::guard_user_file(
            paths,
            directory,
            &format!("{}/{}", item.directory, item.filename),
            false,
        )?;
        if !seen.insert((item.directory.clone(), item.filename.to_lowercase())) {
            return Err(CoreError::InvalidInput);
        }
        let record = before
            .iter()
            .find(|r| {
                r.directory == item.directory
                    && r.file.filename == item.filename
                    && r.file.hashes.sha512 == item.sha512
            })
            .ok_or(CoreError::RecordConflict)?;
        let active = record.directory == "mods";
        let disabled = record.directory == "mods_disabled";
        match request.action {
            ContentAction::Enable if active => continue,
            ContentAction::Disable if disabled => continue,
            ContentAction::Enable | ContentAction::Disable if !active && !disabled => {
                return Err(CoreError::ContentUnsupported);
            }
            _ => (),
        }
        selected.push(record.clone());
    }
    if selected.is_empty() {
        return Ok(());
    }
    let mut after = before.clone();
    for record in &selected {
        match request.action {
            ContentAction::Delete => after.retain(|r| {
                !(r.directory == record.directory && r.file.filename == record.file.filename)
            }),
            _ => {
                let next = after
                    .iter_mut()
                    .find(|r| {
                        r.directory == record.directory && r.file.filename == record.file.filename
                    })
                    .ok_or(CoreError::Integrity)?;
                next.directory = if matches!(request.action, ContentAction::Enable) {
                    "mods"
                } else {
                    "mods_disabled"
                }
                .into();
            }
        }
    }
    install::validate_records(&after)?;
    let token = uuid::Uuid::new_v4().to_string();
    let journal = ChangeJournal {
        schema: 1,
        event: ContentHistory {
            id: token.clone(),
            timestamp: now(),
            action: match request.action {
                ContentAction::Enable => "enable",
                ContentAction::Disable => "disable",
                ContentAction::Delete => "delete",
            }
            .into(),
            titles: selected.iter().map(|r| r.title.clone()).collect(),
        },
        token,
        action: request.action,
        before,
        after,
        selected,
    };
    // Check the ENTIRE batch before publishing the journal or changing any file.
    for (index, record) in journal.selected.iter().enumerate() {
        let source = install::target(paths, directory, record)?;
        let destination = destination(paths, directory, &journal, index, record)?;
        if !install::verified(&source, &record.file)? || destination.exists() {
            return Err(CoreError::ContentConflict);
        }
    }
    let file = directory.join(".sporium/content-change.json");
    if paths.checked(&file)?.exists() {
        return Err(CoreError::ContentConflict);
    }
    write_atomic(paths, &file, &serde_json::to_vec(&journal)?)?;
    recover(paths, directory)
}

fn destination(
    paths: &Paths,
    directory: &Path,
    journal: &ChangeJournal,
    index: usize,
    record: &ContentRecord,
) -> Result<std::path::PathBuf, CoreError> {
    let target = match journal.action {
        ContentAction::Enable => directory.join("mods").join(&record.file.filename),
        ContentAction::Disable => directory.join("mods_disabled").join(&record.file.filename),
        ContentAction::Delete => directory
            .join(".sporium/content-trash")
            .join(&journal.token)
            .join(format!("{index}.bin")),
    };
    paths.checked(&target)
}

pub fn recover(paths: &Paths, directory: &Path) -> Result<(), CoreError> {
    let file = paths.checked(&directory.join(".sporium/content-change.json"))?;
    if !file.exists() {
        return Ok(());
    }
    let journal: ChangeJournal = serde_json::from_slice(&read_limited(paths, &file, 64_000_000)?)?;
    if journal.schema != 1 {
        return Err(CoreError::SchemaTooNew);
    }
    crate::instances::model::valid_id(&journal.token)?;
    install::validate_records(&journal.before)?;
    install::validate_records(&journal.after)?;
    install::validate_records(&journal.selected)?;
    // A damaged journal must not discard unrelated receipt entries or adopt unknown files.
    let mut expected = journal.before.clone();
    for record in &journal.selected {
        let index = expected
            .iter()
            .position(|r| {
                r.directory == record.directory && r.file.filename == record.file.filename
            })
            .ok_or(CoreError::Integrity)?;
        if serde_json::to_vec(&expected[index])? != serde_json::to_vec(record)? {
            return Err(CoreError::Integrity);
        }
        match journal.action {
            ContentAction::Delete => {
                expected.remove(index);
            }
            ContentAction::Enable if record.directory == "mods_disabled" => {
                expected[index].directory = "mods".into()
            }
            ContentAction::Disable if record.directory == "mods" => {
                expected[index].directory = "mods_disabled".into()
            }
            _ => return Err(CoreError::Integrity),
        }
    }
    if serde_json::to_vec(&expected)? != serde_json::to_vec(&journal.after)?
        || journal.event.id != journal.token
    {
        return Err(CoreError::Integrity);
    }
    let receipt = serde_json::to_vec(&install::records(paths, directory)?)?;
    if receipt != serde_json::to_vec(&journal.before)?
        && receipt != serde_json::to_vec(&journal.after)?
    {
        return Err(CoreError::RecordConflict);
    }
    for (index, record) in journal.selected.iter().enumerate() {
        let source = install::target(paths, directory, record)?;
        let destination = destination(paths, directory, &journal, index, record)?;
        if source == destination
            || (!source.exists() && !destination.exists())
            || (source.exists() && !install::verified(&source, &record.file)?)
            || (destination.exists() && !install::verified(&destination, &record.file)?)
        {
            return Err(CoreError::ContentConflict);
        }
    }
    for (index, record) in journal.selected.iter().enumerate() {
        let source = install::target(paths, directory, record)?;
        let destination = destination(paths, directory, &journal, index, record)?;
        if !destination.exists() {
            paths.mkdir(destination.parent().ok_or(CoreError::UnsafePath)?)?;
            let mut temp = tempfile::NamedTempFile::new_in(
                destination.parent().ok_or(CoreError::UnsafePath)?,
            )?;
            std::io::copy(&mut fs::File::open(&source)?, &mut temp)?;
            temp.flush()?;
            temp.as_file().sync_all()?;
            if !install::verified(temp.path(), &record.file)? {
                return Err(CoreError::Integrity);
            }
            paths.checked(&destination)?;
            temp.persist_noclobber(&destination)
                .map_err(|e| CoreError::Io(e.error))?;
        }
        if source.exists() {
            if !install::verified(&source, &record.file)? {
                return Err(CoreError::ContentConflict);
            }
            paths.remove(&source)?;
        }
    }
    install::write_records(paths, directory, &journal.after)?;
    append_history(paths, directory, journal.event)?;
    paths.remove(&file)?;
    // The journal is durable until state AND history are committed. Orphaned trash after
    // a cleanup failure is harmless; it is never scanned or launched as active content.
    if matches!(journal.action, ContentAction::Delete) {
        let _ = paths.remove(
            &directory
                .join(".sporium/content-trash")
                .join(&journal.token),
        );
    }
    Ok(())
}
