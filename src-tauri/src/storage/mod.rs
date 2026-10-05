pub mod migrations;

use rusqlite::{Connection, TransactionBehavior};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    error::CoreError,
    settings::{SaveSettingsRequest, Settings, SettingsSnapshot},
};

/// Connections live only inside background operations; SQLite serializes writers.
/// No filesystem access or database handle is exposed to the webview.
#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
}

impl Database {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn connect(&self) -> Result<Connection, CoreError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut connection = Connection::open(&self.path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        migrations::migrate(&mut connection)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        let defaults = serde_json::to_string(&Settings::default())?;
        connection.execute(
            "INSERT INTO settings(id, revision, payload) VALUES(1, 0, ?1) ON CONFLICT(id) DO NOTHING",
            [defaults],
        )?;
        Ok(connection)
    }

    pub fn load_settings(&self) -> Result<SettingsSnapshot, CoreError> {
        let connection = self.connect()?;
        Self::read(&connection)
    }

    fn read(connection: &Connection) -> Result<SettingsSnapshot, CoreError> {
        let (revision, payload): (u32, String) = connection.query_row(
            "SELECT revision, payload FROM settings WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let document: serde_json::Value = serde_json::from_str(&payload)?;
        if document
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|version| version > crate::settings::SETTINGS_SCHEMA_VERSION.into())
        {
            return Err(CoreError::SchemaTooNew);
        }
        let values: Settings = serde_json::from_value(document)?;
        values.validate()?;
        Ok(SettingsSnapshot { values, revision })
    }

    pub fn save_settings(
        &self,
        request: SaveSettingsRequest,
    ) -> Result<SettingsSnapshot, CoreError> {
        request.values.validate()?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Validate existing data before replacing it: never overwrite unknown future schemas.
        let current = Self::read(&transaction)?;
        if current.revision != request.expected_revision {
            return Err(CoreError::SettingsConflict);
        }
        let revision = current
            .revision
            .checked_add(1)
            .ok_or(CoreError::InvalidSettings)?;
        transaction.execute(
            "UPDATE settings SET revision = ?1, payload = ?2 WHERE id = 1",
            rusqlite::params![revision, serde_json::to_string(&request.values)?],
        )?;
        transaction.commit()?;
        tracing::info!("settings_saved");
        Ok(SettingsSnapshot {
            values: request.values,
            revision,
        })
    }
}
