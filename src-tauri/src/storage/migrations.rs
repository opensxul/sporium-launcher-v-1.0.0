use rusqlite::{Connection, TransactionBehavior};

use crate::error::CoreError;

pub const SCHEMA_VERSION: u32 = 3;

const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/001_settings.sql"),
    include_str!("../../migrations/002_instances.sql"),
    include_str!("../../migrations/003_local_profiles.sql"),
];

pub fn migrate(connection: &mut Connection) -> Result<(), CoreError> {
    apply(connection, MIGRATIONS)
}

fn apply(connection: &mut Connection, migrations: &[&str]) -> Result<(), CoreError> {
    // Acquire the write lock before reading the version, including simultaneous first starts.
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current: u32 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current as usize > migrations.len() {
        return Err(CoreError::SchemaTooNew);
    }
    for (index, migration) in migrations.iter().enumerate().skip(current as usize) {
        transaction.execute_batch(migration)?;
        transaction.pragma_update(None, "user_version", (index + 1) as u32)?;
    }
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_is_repeatable_and_rejects_downgrades() {
        let mut db = Connection::open_in_memory().unwrap();
        migrate(&mut db).unwrap();
        migrate(&mut db).unwrap();
        let version: u32 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        db.pragma_update(None, "user_version", 99).unwrap();
        assert!(matches!(migrate(&mut db), Err(CoreError::SchemaTooNew)));
        let version: u32 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, 99);
    }

    #[test]
    fn phase_one_settings_survive_instance_migration() {
        let mut db = Connection::open_in_memory().unwrap();
        apply(&mut db, &MIGRATIONS[..1]).unwrap();
        let original = r#"{"schemaVersion":1,"locale":"en-US","motion":"reduced","uiScale":125}"#;
        db.execute(
            "INSERT INTO settings(id,revision,payload) VALUES(1,7,?1)",
            [original],
        )
        .unwrap();
        migrate(&mut db).unwrap();
        let (revision, payload): (u32, String) = db
            .query_row(
                "SELECT revision,payload FROM settings WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(revision, 7);
        assert_eq!(payload, original);
        assert_eq!(
            db.query_row("SELECT count(*) FROM instances", [], |row| row
                .get::<_, u32>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn failed_migration_rolls_back_schema_and_version() {
        let mut db = Connection::open_in_memory().unwrap();
        assert!(
            apply(
                &mut db,
                &[MIGRATIONS[0], "CREATE TABLE partial(id); INVALID SQL;"]
            )
            .is_err()
        );
        let version: u32 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, 0);
        let count: u32 = db
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
