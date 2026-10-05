use super::{filesystem::Paths, model::*, repository};
use crate::error::CoreError;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Building,
    Publish,
    Delete,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pending {
    pub schema_version: u32,
    pub phase: Phase,
    pub instance: Instance,
    pub mode: Option<DeleteMode>,
}

pub fn save(db: &Connection, pending: &Pending) -> Result<(), CoreError> {
    db.execute("INSERT INTO instance_operations(id, payload) VALUES(?1, ?2) ON CONFLICT(id) DO UPDATE SET payload = excluded.payload", rusqlite::params![pending.instance.id, serde_json::to_string(pending)?])?;
    Ok(())
}

pub fn recover(db: &mut Connection, paths: &Paths) -> Result<(), CoreError> {
    let pending: Vec<(String, String)> = db
        .prepare("SELECT id, payload FROM instance_operations")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (id, payload) in pending {
        let operation: Pending = repository::decode(&payload)?;
        if operation.schema_version != INSTANCE_SCHEMA || operation.instance.id != id {
            return Err(CoreError::UnsafePath);
        }
        operation.instance.validate()?;
        finish(db, paths, &operation)?;
    }
    Ok(())
}

pub fn finish(db: &mut Connection, paths: &Paths, pending: &Pending) -> Result<(), CoreError> {
    pending.instance.validate()?;
    if (pending.phase == Phase::Delete) != pending.mode.is_some() {
        return Err(CoreError::InvalidInput);
    }
    let id = &pending.instance.id;
    let stage = paths.location("launcher/staging", id)?;
    let installed = paths.location("instances", id)?;
    match pending.phase {
        Phase::Building => {
            // The copy was never marked complete. Original instance remains untouched.
            paths.remove(&stage)?;
        }
        Phase::Publish => {
            if !installed.exists() {
                paths.verify_marker(&stage, id)?;
                paths.rename(&stage, &installed)?;
            } else if stage.exists() {
                return Err(CoreError::UnsafePath);
            }
            paths.verify_marker(&installed, id)?;
            let transaction = db.transaction()?;
            repository::insert_instance(&transaction, &pending.instance)?;
            transaction.execute("DELETE FROM instance_operations WHERE id = ?1", [id])?;
            transaction.commit()?;
            return Ok(());
        }
        Phase::Delete => {
            let trash = paths.location("launcher/trash", id)?;
            let backup = paths.location("backups", id)?;
            if !installed.exists()
                && !trash.exists()
                && !backup.exists()
                && db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM instances WHERE id = ?1)",
                    [id],
                    |row| row.get::<_, bool>(0),
                )?
            {
                return Err(CoreError::NotFound);
            }
            if installed.exists() {
                if backup.exists() || trash.exists() {
                    return Err(CoreError::UnsafePath);
                }
                paths.verify_marker(&installed, id)?;
                paths.check_tree(&installed, 0)?;
                paths.rename(&installed, &trash)?;
            }
            // Delete the catalog record only after the directory has been detached.
            db.execute("DELETE FROM instances WHERE id = ?1", [id])?;
            match pending.mode.ok_or(CoreError::InvalidInput)? {
                DeleteMode::Everything => paths.remove(&trash)?,
                DeleteMode::PreserveWorlds => {
                    if trash.exists() {
                        if backup.exists() {
                            return Err(CoreError::UnsafePath);
                        }
                        paths.rename(&trash, &backup)?;
                    }
                    paths.verify_marker(&backup, id)?;
                    paths.write_json(&backup.join("instance-backup.json"), &pending.instance)?;
                    for entry in fs::read_dir(&backup)? {
                        let entry = entry?;
                        if ![
                            "saves",
                            "screenshots",
                            "instance.json",
                            "instance-backup.json",
                        ]
                        .iter()
                        .any(|name| entry.file_name() == *name)
                        {
                            paths.remove(&entry.path())?;
                        }
                    }
                }
            }
        }
    }
    db.execute("DELETE FROM instance_operations WHERE id = ?1", [id])?;
    Ok(())
}
