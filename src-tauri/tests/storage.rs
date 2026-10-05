use sporium_lib::{
    error::{CommandError, CoreError, ErrorCode},
    settings::{Locale, Motion, SaveSettingsRequest, Settings},
    storage::Database,
};

fn database(directory: &tempfile::TempDir) -> Database {
    Database::new(directory.path().join("launcher/sporium.sqlite3"))
}

#[test]
fn fresh_defaults_and_changes_survive_new_database_instance() {
    let directory = tempfile::tempdir().unwrap();
    let db = database(&directory);
    let fresh = db.load_settings().unwrap();
    assert_eq!(fresh.values, Settings::default());
    assert_eq!(fresh.revision, 0);
    let mut changed = fresh.values;
    changed.locale = Locale::English;
    changed.motion = Motion::Reduced;
    changed.ui_scale = 125;
    let saved = db
        .save_settings(SaveSettingsRequest {
            values: changed,
            expected_revision: fresh.revision,
        })
        .unwrap();
    drop(db);
    assert_eq!(database(&directory).load_settings().unwrap(), saved);
    assert_eq!(saved.revision, 1);
}

#[test]
fn stale_writes_are_rejected_without_losing_settings() {
    let directory = tempfile::tempdir().unwrap();
    let db = database(&directory);
    let original = db.load_settings().unwrap();
    let mut values = original.values.clone();
    values.locale = Locale::English;
    db.save_settings(SaveSettingsRequest {
        values,
        expected_revision: 0,
    })
    .unwrap();
    assert!(matches!(
        db.save_settings(SaveSettingsRequest {
            values: original.values,
            expected_revision: 0
        }),
        Err(CoreError::SettingsConflict)
    ));
    assert_eq!(db.load_settings().unwrap().values.locale, Locale::English);
}

#[test]
fn concurrent_writes_have_exactly_one_winner() {
    let directory = tempfile::tempdir().unwrap();
    let db = database(&directory);
    db.load_settings().unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = [Locale::Russian, Locale::English]
        .into_iter()
        .map(|locale| {
            let db = db.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                db.save_settings(SaveSettingsRequest {
                    values: Settings {
                        locale,
                        ..Settings::default()
                    },
                    expected_revision: 0,
                })
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(CoreError::SettingsConflict)))
            .count(),
        1
    );
}

#[test]
fn invalid_values_and_newer_settings_cannot_overwrite_saved_data() {
    let directory = tempfile::tempdir().unwrap();
    let db = database(&directory);
    let before = db.load_settings().unwrap();
    for (schema_version, ui_scale) in [(1, 999), (0, 100), (99, 100)] {
        let request = SaveSettingsRequest {
            values: Settings {
                schema_version,
                ui_scale,
                ..Settings::default()
            },
            expected_revision: 0,
        };
        assert!(db.save_settings(request).is_err());
        assert_eq!(db.load_settings().unwrap(), before);
    }
}

#[test]
fn corrupt_data_is_preserved_and_error_does_not_expose_private_data() {
    let directory = tempfile::tempdir().unwrap();
    let db = database(&directory);
    db.load_settings().unwrap();
    let connection = rusqlite::Connection::open(db.path()).unwrap();
    connection
        .execute(
            "UPDATE settings SET payload = ?1",
            [r#"{"private":"sensitive-value"}"#],
        )
        .unwrap();
    let error: CommandError = db.load_settings().unwrap_err().into();
    assert_eq!(error.code, ErrorCode::DataCorrupt);
    assert!(
        !serde_json::to_string(&error)
            .unwrap()
            .contains("sensitive-value")
    );
    assert!(
        db.save_settings(SaveSettingsRequest {
            values: Settings::default(),
            expected_revision: 0
        })
        .is_err()
    );
    let preserved: String = connection
        .query_row("SELECT payload FROM settings", [], |r| r.get(0))
        .unwrap();
    assert!(preserved.contains("sensitive-value"));
}

#[test]
fn unknown_fields_cannot_be_smuggled_into_settings() {
    let json = r#"{"schemaVersion":1,"locale":"ru-RU","motion":"system","uiScale":100,"token":"do-not-store"}"#;
    assert!(serde_json::from_str::<Settings>(json).is_err());
}

#[test]
fn future_database_is_not_reset() {
    let directory = tempfile::tempdir().unwrap();
    let db = database(&directory);
    let before = db.load_settings().unwrap();
    let connection = rusqlite::Connection::open(db.path()).unwrap();
    connection.pragma_update(None, "user_version", 999).unwrap();
    assert!(matches!(db.load_settings(), Err(CoreError::SchemaTooNew)));
    let revision: u32 = connection
        .query_row("SELECT revision FROM settings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(revision, before.revision);
}

#[test]
fn future_settings_schema_is_recognized_even_with_unknown_fields() {
    let directory = tempfile::tempdir().unwrap();
    let db = database(&directory);
    db.load_settings().unwrap();
    let connection = rusqlite::Connection::open(db.path()).unwrap();
    let payload = r#"{"schemaVersion":99,"locale":"new-locale","newFutureField":true}"#;
    connection
        .execute("UPDATE settings SET payload = ?1", [payload])
        .unwrap();
    assert!(matches!(db.load_settings(), Err(CoreError::SchemaTooNew)));
    assert!(matches!(
        db.save_settings(SaveSettingsRequest {
            values: Settings::default(),
            expected_revision: 0
        }),
        Err(CoreError::SchemaTooNew)
    ));
    let unchanged: String = connection
        .query_row("SELECT payload FROM settings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(unchanged, payload);
}
