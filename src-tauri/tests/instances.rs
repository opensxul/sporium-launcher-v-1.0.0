use sporium_lib::{
    error::CoreError,
    instances::{Library, model::*},
    storage::Database,
};

fn setup() -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Sporium");
    let db = Database::new(root.join("launcher/sporium.sqlite3"));
    (dir, Library::new(root, db))
}

#[test]
fn summary_counts_real_jar_files_and_nested_bytes_without_shared_assets() {
    let (_dir, library) = setup();
    let instance = create(&library, "Metrics");
    let root = library.root().join("instances").join(&instance.id);
    let before = library.summary(&instance.id).unwrap();
    std::fs::write(root.join("mods/one.jar"), b"12345").unwrap();
    std::fs::write(root.join("mods_disabled/two.JAR"), b"1234").unwrap();
    std::fs::write(root.join("mods/readme.txt"), b"123").unwrap();
    std::fs::create_dir(root.join("saves/nested")).unwrap();
    std::fs::write(root.join("saves/nested/test.dat"), b"12").unwrap();
    std::fs::write(library.root().join("shared/unrelated.dat"), b"excluded").unwrap();
    let after = library.summary(&instance.id).unwrap();
    assert_eq!(after.enabled_mods, 1);
    assert_eq!(after.disabled_mods, 1);
    assert_eq!(
        after.directory_bytes.unwrap() - before.directory_bytes.unwrap(),
        14
    );
}

#[test]
fn active_game_lease_prevents_delete_and_duplicate() {
    let (_dir, library) = setup();
    let instance = create(&library, "Busy world");
    let lock_path = library
        .root()
        .join("launcher/instance-locks")
        .join(format!("{}.lock", instance.id));
    let guard = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&guard).unwrap();
    assert!(matches!(
        library.delete(DeleteInstance {
            id: instance.id.clone(),
            expected_revision: instance.revision,
            mode: DeleteMode::Everything
        }),
        Err(CoreError::InstanceBusy)
    ));
    assert!(matches!(
        library.duplicate(DuplicateInstance {
            id: instance.id.clone(),
            expected_revision: instance.revision,
            name: "Copy".into()
        }),
        Err(CoreError::InstanceBusy)
    ));
    assert!(
        library
            .root()
            .join("instances")
            .join(&instance.id)
            .join("instance.json")
            .exists()
    );
    drop(guard);
    library
        .delete(DeleteInstance {
            id: instance.id,
            expected_revision: instance.revision,
            mode: DeleteMode::Everything,
        })
        .unwrap();
}

fn create(library: &Library, name: &str) -> Instance {
    let result = library
        .create(CreateInstance {
            name: name.into(),
            minecraft_version: "1.21.4".into(),
            loader: Loader::Fabric,
            collection_id: None,
        })
        .unwrap();
    result
        .snapshot
        .instances
        .into_iter()
        .find(|i| i.id == result.affected_id)
        .unwrap()
}

#[test]
fn custom_icons_survive_restart_and_duplicate_reject_stale_changes_and_reset() {
    let (dir, library) = setup();
    let instance = create(&library, "Icon test");
    let source = dir.path().join("user-icon.png");
    image::RgbaImage::from_pixel(32, 32, image::Rgba([20, 200, 80, 128]))
        .save(&source)
        .unwrap();
    let request = RecordRequest {
        id: instance.id.clone(),
        expected_revision: instance.revision,
    };
    let changed = library.custom_icon(request.clone(), &source).unwrap();
    let updated = changed
        .snapshot
        .instances
        .iter()
        .find(|value| value.id == instance.id)
        .unwrap();
    assert_eq!(updated.icon_source, IconSource::Custom);
    assert!(updated.icon_ref.as_ref().unwrap().starts_with("custom:"));
    assert!(library.custom_icon(request, &source).is_err());
    std::fs::remove_file(source).unwrap();
    let root = library.root().to_path_buf();
    let restarted = Library::new(
        root.clone(),
        Database::new(root.join("launcher/sporium.sqlite3")),
    );
    let icon = restarted.icon_image(&instance.id).unwrap().unwrap();
    assert!(icon.starts_with("data:image/png;base64,"));
    let copy = restarted
        .duplicate(DuplicateInstance {
            id: instance.id.clone(),
            expected_revision: updated.revision,
            name: "Icon copy".into(),
        })
        .unwrap();
    assert_eq!(
        restarted.icon_image(&copy.affected_id).unwrap().as_deref(),
        Some(icon.as_str())
    );
    let reset = restarted
        .select_icon(
            RecordRequest {
                id: instance.id.clone(),
                expected_revision: updated.revision,
            },
            "automatic",
        )
        .unwrap();
    assert_eq!(
        reset
            .snapshot
            .instances
            .iter()
            .find(|value| value.id == instance.id)
            .unwrap()
            .icon_source,
        IconSource::Automatic
    );
    assert!(restarted.icon_image(&instance.id).unwrap().is_none());
    assert_eq!(
        restarted.icon_image(&copy.affected_id).unwrap().as_deref(),
        Some(icon.as_str())
    );
}

#[test]
fn corrupt_icon_cannot_replace_selection_and_cache_tampering_is_detected() {
    let (dir, library) = setup();
    let instance = create(&library, "Protected icon");
    let source = dir.path().join("icon.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([40, 100, 80, 255]))
        .save(&source)
        .unwrap();
    let change = library
        .custom_icon(
            RecordRequest {
                id: instance.id.clone(),
                expected_revision: instance.revision,
            },
            &source,
        )
        .unwrap();
    let updated = &change.snapshot.instances[0];
    std::fs::write(&source, b"broken png").unwrap();
    assert!(
        library
            .custom_icon(
                RecordRequest {
                    id: instance.id.clone(),
                    expected_revision: updated.revision
                },
                &source
            )
            .is_err()
    );
    assert!(library.icon_image(&instance.id).unwrap().is_some());
    let hash = updated
        .icon_ref
        .as_ref()
        .unwrap()
        .strip_prefix("custom:")
        .unwrap();
    std::fs::write(
        library
            .root()
            .join("shared/instance-icons")
            .join(format!("{hash}.png")),
        b"changed",
    )
    .unwrap();
    assert!(matches!(
        library.icon_image(&instance.id),
        Err(CoreError::Integrity)
    ));
    assert_eq!(
        library
            .folder(OpenFolder {
                id: Some(instance.id),
                target: FolderTarget::Logs
            })
            .unwrap()
            .file_name()
            .unwrap(),
        "logs"
    );
}

#[test]
fn instances_are_isolated_and_use_uuids_not_display_names() {
    let (_dir, lib) = setup();
    let a = create(&lib, "../../display name");
    let b = create(&lib, "Вторая сборка");
    assert_ne!(a.id, b.id);
    assert!(uuid::Uuid::parse_str(&a.id).is_ok());
    let root_a = lib
        .folder(OpenFolder {
            target: FolderTarget::Instance,
            id: Some(a.id.clone()),
        })
        .unwrap();
    let root_b = lib
        .folder(OpenFolder {
            target: FolderTarget::Instance,
            id: Some(b.id.clone()),
        })
        .unwrap();
    for name in [
        "mods",
        "mods_disabled",
        "config",
        "saves",
        "resourcepacks",
        "shaderpacks",
        "screenshots",
        "logs",
    ] {
        assert!(root_a.join(name).is_dir());
        assert!(root_b.join(name).is_dir());
    }
    std::fs::write(root_a.join("mods/only-a.jar"), "A").unwrap();
    assert!(!root_b.join("mods/only-a.jar").exists());
    assert_eq!(a.status, InstallStatus::NotInstalled);
    assert_eq!(a.java_mode, AutoMode::Auto);
}

#[test]
fn duplicate_copies_bytes_and_preserves_original_worlds() {
    let (_dir, lib) = setup();
    let a = create(&lib, "Original");
    let root_a = lib
        .folder(OpenFolder {
            target: FolderTarget::Instance,
            id: Some(a.id.clone()),
        })
        .unwrap();
    std::fs::create_dir(root_a.join("saves/world")).unwrap();
    std::fs::write(root_a.join("saves/world/level.dat"), b"world-state").unwrap();
    std::fs::write(root_a.join("mods/mod.jar"), b"original-mod").unwrap();
    let result = lib
        .duplicate(DuplicateInstance {
            id: a.id.clone(),
            expected_revision: a.revision,
            name: "Copy".into(),
        })
        .unwrap();
    let root_b = lib
        .folder(OpenFolder {
            target: FolderTarget::Instance,
            id: Some(result.affected_id.clone()),
        })
        .unwrap();
    assert_eq!(
        std::fs::read(root_b.join("saves/world/level.dat")).unwrap(),
        b"world-state"
    );
    std::fs::write(root_b.join("mods/mod.jar"), b"changed-copy").unwrap();
    assert_eq!(
        std::fs::read(root_a.join("mods/mod.jar")).unwrap(),
        b"original-mod"
    );
    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root_b.join("instance.json")).unwrap()).unwrap();
    assert_eq!(marker["id"], result.affected_id);
}

#[test]
fn rename_and_collections_are_metadata_only_and_survive_restart() {
    let (_dir, lib) = setup();
    let a = create(&lib, "Before");
    let folder = lib
        .save_collection(SaveCollection {
            id: None,
            expected_revision: None,
            name: "С друзьями".into(),
            description: "Наш мир".into(),
            accent: Accent::Sage,
            icon: CollectionIcon::Users,
        })
        .unwrap();
    let updated = lib
        .update(UpdateInstance {
            id: a.id.clone(),
            expected_revision: 0,
            name: "После".into(),
            collection_id: Some(folder.affected_id.clone()),
        })
        .unwrap();
    let instance = updated
        .snapshot
        .instances
        .iter()
        .find(|i| i.id == a.id)
        .unwrap();
    assert_eq!(instance.name, "После");
    assert_eq!(instance.collection_id, Some(folder.affected_id.clone()));
    assert!(lib.root().join("instances").join(&a.id).is_dir());
    assert!(matches!(
        lib.update(UpdateInstance {
            id: a.id.clone(),
            expected_revision: 0,
            name: "stale".into(),
            collection_id: None
        }),
        Err(CoreError::RecordConflict)
    ));
    let other = Library::new(
        lib.root().to_path_buf(),
        Database::new(lib.root().join("launcher/sporium.sqlite3")),
    );
    assert_eq!(other.snapshot().unwrap().instances[0].name, "После");
    let deleted = other
        .delete_collection(RecordRequest {
            id: folder.affected_id,
            expected_revision: 0,
        })
        .unwrap();
    assert!(deleted.snapshot.instances[0].collection_id.is_none());
    assert!(other.root().join("instances").join(&a.id).is_dir());
}

#[test]
fn deleting_with_preservation_keeps_worlds_and_screenshots() {
    let (_dir, lib) = setup();
    let a = create(&lib, "Worlds");
    let root = lib.root().join("instances").join(&a.id);
    std::fs::write(root.join("saves/world.dat"), b"keep-world").unwrap();
    std::fs::write(root.join("screenshots/photo.png"), b"keep-image").unwrap();
    std::fs::write(root.join("mods/mod.jar"), b"remove-mod").unwrap();
    let result = lib
        .delete(DeleteInstance {
            id: a.id.clone(),
            expected_revision: 0,
            mode: DeleteMode::PreserveWorlds,
        })
        .unwrap();
    assert!(result.snapshot.instances.is_empty());
    assert!(!root.exists());
    let archive = std::path::PathBuf::from(result.preserved_directory.unwrap());
    assert_eq!(
        std::fs::read(archive.join("saves/world.dat")).unwrap(),
        b"keep-world"
    );
    assert_eq!(
        std::fs::read(archive.join("screenshots/photo.png")).unwrap(),
        b"keep-image"
    );
    assert!(!archive.join("mods").exists());
    assert!(archive.join("instance-backup.json").is_file());
}

#[test]
fn full_delete_affects_only_selected_instance_and_refuses_invalid_ids() {
    let (_dir, lib) = setup();
    let a = create(&lib, "Delete");
    let b = create(&lib, "Keep");
    assert!(
        lib.delete(DeleteInstance {
            id: "../../".into(),
            expected_revision: 0,
            mode: DeleteMode::Everything
        })
        .is_err()
    );
    lib.delete(DeleteInstance {
        id: a.id.clone(),
        expected_revision: 0,
        mode: DeleteMode::Everything,
    })
    .unwrap();
    assert!(!lib.root().join("instances").join(a.id).exists());
    assert!(lib.root().join("instances").join(b.id).exists());
    assert_eq!(lib.snapshot().unwrap().instances.len(), 1);
}

#[test]
fn invalid_metadata_does_not_create_a_directory_or_record() {
    let (_dir, lib) = setup();
    for (name, version) in [
        ("", "1.21.4"),
        ("ok", "../../escape"),
        ("bad\nname", "1.21.4"),
    ] {
        assert!(
            lib.create(CreateInstance {
                name: name.into(),
                minecraft_version: version.into(),
                loader: Loader::Vanilla,
                collection_id: None
            })
            .is_err()
        );
    }
    assert!(lib.snapshot().unwrap().instances.is_empty());
}

#[cfg(windows)]
#[test]
fn junctions_cannot_escape_the_instance_root_during_copy_delete_or_open() {
    use std::process::Command;
    let (dir, lib) = setup();
    let a = create(&lib, "Linked");
    let outside = dir.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("precious.txt"), "safe").unwrap();
    let mods = lib.root().join("instances").join(&a.id).join("mods");
    std::fs::remove_dir(&mods).unwrap();
    // Fixture only: fixed mklink operation, independent path arguments; never delete through cmd.
    let output = Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(&mods)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "could not create the junction fixture"
    );
    assert!(matches!(
        lib.duplicate(DuplicateInstance {
            id: a.id.clone(),
            expected_revision: 0,
            name: "Copy".into()
        }),
        Err(CoreError::UnsafePath)
    ));
    assert!(matches!(
        lib.delete(DeleteInstance {
            id: a.id.clone(),
            expected_revision: 0,
            mode: DeleteMode::Everything
        }),
        Err(CoreError::UnsafePath)
    ));
    assert!(matches!(
        lib.folder(OpenFolder {
            target: FolderTarget::Mods,
            id: Some(a.id)
        }),
        Err(CoreError::UnsafePath)
    ));
    assert_eq!(
        std::fs::read_to_string(outside.join("precious.txt")).unwrap(),
        "safe"
    );
    std::fs::remove_dir(&mods).unwrap(); // unlink the junction itself
}
