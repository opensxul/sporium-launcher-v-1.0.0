use rusqlite::{Connection, params};
use serde_json::json;
use sporium_lib::{
    error::CoreError,
    instances::{Library, model::*},
    storage::Database,
};
use std::fs;

fn setup() -> (tempfile::TempDir, Library, Instance) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Sporium");
    let lib = Library::new(
        root.clone(),
        Database::new(root.join("launcher/sporium.sqlite3")),
    );
    let created = lib
        .create(CreateInstance {
            name: "Original".into(),
            minecraft_version: "1.21.4".into(),
            loader: Loader::Vanilla,
            collection_id: None,
        })
        .unwrap();
    (dir, lib, created.snapshot.instances[0].clone())
}

fn connection(lib: &Library) -> Connection {
    Connection::open(lib.root().join("launcher/sporium.sqlite3")).unwrap()
}

fn pending(lib: &Library, instance: &Instance, phase: &str, mode: Option<&str>) {
    connection(lib)
        .execute(
            "INSERT INTO instance_operations(id,payload) VALUES(?1,?2)",
            params![
                instance.id,
                json!({"schemaVersion":1,"phase":phase,"instance":instance,"mode":mode})
                    .to_string()
            ],
        )
        .unwrap();
}

#[test]
fn incomplete_copy_is_discarded_and_complete_publish_recovers_on_either_side_of_rename() {
    for phase in ["building", "publish-before-rename", "publish-after-rename"] {
        let (_dir, lib, original) = setup();
        let original_path = lib.root().join("instances").join(&original.id);
        fs::write(original_path.join("saves/original.dat"), "precious").unwrap();
        let mut copy = original.clone();
        copy.id = uuid::Uuid::new_v4().to_string();
        let stage = lib.root().join("launcher/staging").join(&copy.id);
        fs::create_dir(&stage).unwrap();
        fs::write(
            stage.join("instance.json"),
            json!({"schemaVersion":1,"id":copy.id}).to_string(),
        )
        .unwrap();
        fs::write(stage.join("copied-file"), "payload").unwrap();
        pending(
            &lib,
            &copy,
            if phase == "building" {
                "building"
            } else {
                "publish"
            },
            None,
        );
        let installed = lib.root().join("instances").join(&copy.id);
        if phase == "publish-after-rename" {
            fs::rename(&stage, &installed).unwrap();
        }
        let recovered = lib.snapshot().unwrap();
        assert_eq!(
            recovered.instances.len(),
            if phase == "building" { 1 } else { 2 }
        );
        assert!(!stage.exists());
        if phase != "building" {
            assert_eq!(
                fs::read_to_string(installed.join("copied-file")).unwrap(),
                "payload"
            );
        }
        assert_eq!(
            fs::read_to_string(original_path.join("saves/original.dat")).unwrap(),
            "precious"
        );
        assert_eq!(
            lib.snapshot().unwrap().instances.len(),
            recovered.instances.len()
        );
        assert_eq!(
            connection(&lib)
                .query_row("SELECT count(*) FROM instance_operations", [], |row| row
                    .get::<_, u32>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn preserve_delete_recovers_before_and_after_detach_and_database_removal() {
    for step in 0..4 {
        let (_dir, lib, instance) = setup();
        let installed = lib.root().join("instances").join(&instance.id);
        fs::write(installed.join("saves/world.dat"), "world").unwrap();
        fs::write(installed.join("screenshots/image.png"), "image").unwrap();
        fs::write(installed.join("mods/mod.jar"), "mod").unwrap();
        pending(&lib, &instance, "delete", Some("preserve_worlds"));
        let trash = lib.root().join("launcher/trash").join(&instance.id);
        let backup = lib.root().join("backups").join(&instance.id);
        if step >= 1 {
            fs::rename(&installed, &trash).unwrap();
        }
        if step >= 2 {
            connection(&lib)
                .execute("DELETE FROM instances WHERE id=?1", [&instance.id])
                .unwrap();
        }
        if step >= 3 {
            fs::rename(&trash, &backup).unwrap();
        }
        assert!(lib.snapshot().unwrap().instances.is_empty());
        assert_eq!(
            fs::read_to_string(backup.join("saves/world.dat")).unwrap(),
            "world"
        );
        assert_eq!(
            fs::read_to_string(backup.join("screenshots/image.png")).unwrap(),
            "image"
        );
        assert!(!backup.join("mods").exists());
        assert!(lib.snapshot().unwrap().instances.is_empty());
    }
}

#[test]
fn malformed_delete_journal_cannot_modify_instance() {
    let (_dir, lib, instance) = setup();
    pending(&lib, &instance, "delete", None);
    assert!(matches!(lib.snapshot(), Err(CoreError::InvalidInput)));
    assert!(lib.root().join("instances").join(&instance.id).exists());
    assert_eq!(
        connection(&lib)
            .query_row("SELECT count(*) FROM instances", [], |row| row
                .get::<_, u32>(0))
            .unwrap(),
        1
    );
}

#[test]
fn concurrent_library_access_fails_without_changing_records() {
    let (_dir, lib, instance) = setup();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lib.root().join("launcher/instances.lock"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    assert!(matches!(
        lib.delete(DeleteInstance {
            id: instance.id,
            expected_revision: 0,
            mode: DeleteMode::Everything
        }),
        Err(CoreError::LibraryBusy)
    ));
    fs2::FileExt::unlock(&lock).unwrap();
    assert_eq!(lib.snapshot().unwrap().instances.len(), 1);
}

#[test]
fn favorite_and_shortcut_identity_survive_rename_and_legacy_metadata() {
    let (_dir, lib, instance) = setup();
    // Records created before the v2.5 additions remain readable without rewriting user data.
    let mut old = serde_json::to_value(&instance).unwrap();
    for key in ["favorite", "iconSource", "managedShortcut"] {
        old.as_object_mut().unwrap().remove(key);
    }
    connection(&lib)
        .execute(
            "UPDATE instances SET payload=?1 WHERE id=?2",
            params![old.to_string(), instance.id],
        )
        .unwrap();
    assert!(!lib.snapshot().unwrap().instances[0].favorite);
    lib.set_favorite(SetFavorite {
        id: instance.id.clone(),
        expected_revision: 0,
        favorite: true,
    })
    .unwrap();
    assert!(matches!(
        lib.set_favorite(SetFavorite {
            id: instance.id.clone(),
            expected_revision: 0,
            favorite: false
        }),
        Err(CoreError::RecordConflict)
    ));
    let first = lib
        .shortcut_plan(RecordRequest {
            id: instance.id.clone(),
            expected_revision: 1,
        })
        .unwrap();
    lib.update(UpdateInstance {
        id: instance.id.clone(),
        expected_revision: 1,
        name: "New name".into(),
        collection_id: None,
    })
    .unwrap();
    let renamed = lib
        .shortcut_plan(RecordRequest {
            id: instance.id.clone(),
            expected_revision: 2,
        })
        .unwrap();
    assert_eq!(first.arguments, renamed.arguments);
    assert_eq!(renamed.arguments, ["--launch-instance", &instance.id]);
    assert!(!renamed.available);
    assert!(lib.snapshot().unwrap().instances[0].favorite);
}
