use super::model::*;
use crate::error::CoreError;
use rusqlite::{Connection, OptionalExtension};
use serde::de::DeserializeOwned;

pub fn decode<T: DeserializeOwned>(payload: &str) -> Result<T, CoreError> {
    let value: serde_json::Value = serde_json::from_str(payload)?;
    if value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|version| version > INSTANCE_SCHEMA.into())
    {
        return Err(CoreError::SchemaTooNew);
    }
    Ok(serde_json::from_value(value)?)
}

pub fn snapshot(db: &Connection) -> Result<LibrarySnapshot, CoreError> {
    let mut instances = Vec::new();
    let mut query = db.prepare("SELECT id, collection_id, payload FROM instances ORDER BY json_extract(payload, '$.createdAt'), id")?;
    let mut rows = query.query([])?;
    while let Some(row) = rows.next()? {
        let value: Instance = decode(&row.get::<_, String>(2)?)?;
        value.validate()?;
        if value.id != row.get::<_, String>(0)?
            || value.collection_id != row.get::<_, Option<String>>(1)?
        {
            return Err(CoreError::InvalidInput);
        }
        instances.push(value);
    }
    let mut collections = Vec::new();
    let mut query = db.prepare(
        "SELECT id, payload FROM collections ORDER BY json_extract(payload, '$.order'), id",
    )?;
    let mut rows = query.query([])?;
    while let Some(row) = rows.next()? {
        let value: Collection = decode(&row.get::<_, String>(1)?)?;
        valid_id(&value.id)?;
        if value.id != row.get::<_, String>(0)? {
            return Err(CoreError::InvalidInput);
        }
        collections.push(value);
    }
    Ok(LibrarySnapshot {
        instances,
        collections,
    })
}

pub fn instance(db: &Connection, id: &str) -> Result<Instance, CoreError> {
    valid_id(id)?;
    let payload: String = db
        .query_row("SELECT payload FROM instances WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .optional()?
        .ok_or(CoreError::NotFound)?;
    let value: Instance = decode(&payload)?;
    value.validate()?;
    if value.id != id {
        return Err(CoreError::UnsafePath);
    }
    Ok(value)
}

pub fn collection(db: &Connection, id: &str) -> Result<Collection, CoreError> {
    valid_id(id)?;
    let payload: String = db
        .query_row(
            "SELECT payload FROM collections WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(CoreError::NotFound)?;
    let value: Collection = decode(&payload)?;
    if value.id != id {
        return Err(CoreError::InvalidInput);
    }
    Ok(value)
}

pub fn require_collection(db: &Connection, id: &Option<String>) -> Result<(), CoreError> {
    if let Some(id) = id {
        collection(db, id)?;
    }
    Ok(())
}

pub fn insert_instance(db: &Connection, value: &Instance) -> Result<(), CoreError> {
    value.validate()?;
    db.execute(
        "INSERT INTO instances(id, collection_id, payload) VALUES(?1, ?2, ?3)",
        rusqlite::params![value.id, value.collection_id, serde_json::to_string(value)?],
    )?;
    Ok(())
}

pub fn update_instance(db: &Connection, value: &Instance) -> Result<(), CoreError> {
    value.validate()?;
    db.execute(
        "UPDATE instances SET collection_id = ?2, payload = ?3 WHERE id = ?1",
        rusqlite::params![value.id, value.collection_id, serde_json::to_string(value)?],
    )?;
    Ok(())
}

pub fn revision(actual: u32, expected: u32) -> Result<(), CoreError> {
    if actual != expected {
        return Err(CoreError::RecordConflict);
    }
    Ok(())
}

pub fn change(
    db: &Connection,
    id: String,
    preserved_directory: Option<String>,
) -> Result<LibraryChange, CoreError> {
    Ok(LibraryChange {
        snapshot: snapshot(db)?,
        affected_id: id,
        preserved_directory,
    })
}
