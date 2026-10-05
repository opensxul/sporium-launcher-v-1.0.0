use super::*;
use crate::{
    instances::model::{CreateInstance, Loader},
    storage::Database,
};

fn fixture(root: &Path) -> (Library, Instance, ProjectManager, Paths, PathBuf) {
    let library = Library::new(
        root.into(),
        Database::new(root.join("launcher/sporium.sqlite3")),
    );
    library
        .create(CreateInstance {
            name: "Managed fixture".into(),
            minecraft_version: "1.21.1".into(),
            loader: Loader::Vanilla,
            collection_id: None,
        })
        .unwrap();
    let instance = library.snapshot().unwrap().instances.remove(0);
    let (directory, lease) = library.lease_content_read(&instance.id).unwrap();
    drop(lease);
    let manager = ProjectManager::new(library.clone(), PackManager::new(library.clone())).unwrap();
    let paths = Paths::new(root).unwrap();
    (library, instance, manager, paths, directory)
}
fn studio(manager: &ProjectManager, id: &str, version: &str) -> ProjectPlan {
    let files = manager.studio_files(id).unwrap();
    manager
        .studio_plan(StudioRequest {
            id: id.into(),
            name: "Custom project".into(),
            version: version.into(),
            forbid_external_mods: false,
            policies: files
                .into_iter()
                .map(|file| StudioFilePolicy {
                    path: file.path,
                    policy: file.policy,
                    group: file.group,
                })
                .collect(),
        })
        .unwrap()
}

#[test]
fn studio_preview_commit_locked_repair_and_undo_preserve_user_data() {
    let temp = tempfile::tempdir().unwrap();
    let (library, instance, manager, paths, dir) = fixture(temp.path());
    fs::write(dir.join("resourcepacks/owned.zip"), b"owned original").unwrap();
    fs::write(dir.join("options.txt"), b"old setting").unwrap();
    fs::write(dir.join("screenshots/user.png"), b"user screenshot").unwrap();
    fs::create_dir_all(dir.join("saves/user-world")).unwrap();
    fs::write(dir.join("saves/user-world/level.dat"), b"user world").unwrap();
    let plan = studio(&manager, &instance.id, "1.0");
    assert!(manifest(&paths, &dir).unwrap().is_none());
    manager.apply(&plan.token, true).unwrap();
    assert!(manifest(&paths, &dir).unwrap().unwrap().creator_studio);
    assert!(guard_user_file(&paths, &dir, "resourcepacks/owned.zip", false).is_err());
    assert!(validate_launch(&paths, &dir, &instance).is_ok());
    fs::write(dir.join("resourcepacks/owned.zip"), b"changed").unwrap();
    assert!(validate_launch(&paths, &dir, &instance).is_err());
    let plan = manager.repair_plan(&instance.id).unwrap();
    assert!(plan.warnings.iter().any(|w| w.starts_with("modified:")));
    assert!(manager.apply(&plan.token, false).is_err());
    let point = manager.apply(&plan.token, true).unwrap();
    assert_eq!(
        fs::read(dir.join("resourcepacks/owned.zip")).unwrap(),
        b"owned original"
    );
    fs::write(dir.join("options.txt"), b"new setting").unwrap();
    let (_, _, lease) = library.lease_game(&instance.id).unwrap();
    let undo = transaction::restore(&paths, &dir, &instance, &point, true).unwrap();
    assert_eq!(
        fs::read(dir.join("resourcepacks/owned.zip")).unwrap(),
        b"changed"
    );
    assert_eq!(fs::read(dir.join("options.txt")).unwrap(), b"old setting");
    transaction::restore(&paths, &dir, &instance, &undo, true).unwrap();
    assert_eq!(fs::read(dir.join("options.txt")).unwrap(), b"new setting");
    assert_eq!(
        fs::read(dir.join("saves/user-world/level.dat")).unwrap(),
        b"user world"
    );
    assert_eq!(
        fs::read(dir.join("screenshots/user.png")).unwrap(),
        b"user screenshot"
    );
    drop(lease);
}

#[test]
fn interrupted_transaction_recovers_missing_and_partial_targets_idempotently() {
    let temp = tempfile::tempdir().unwrap();
    let (_, instance, _, paths, dir) = fixture(temp.path());
    fs::write(dir.join("config/a.txt"), b"before-a").unwrap();
    fs::write(dir.join("config/b.txt"), b"before-b").unwrap();
    let a = paths.root().join("a.txt");
    let b = paths.root().join("b.txt");
    fs::write(&a, b"after-a").unwrap();
    fs::write(&b, b"after-b").unwrap();
    let point = transaction::apply(
        &paths,
        &dir,
        &instance,
        BTreeMap::from([
            ("config/a.txt".into(), Some(a)),
            ("config/b.txt".into(), Some(b)),
        ]),
        "project_update",
        vec![],
    )
    .unwrap();
    fs::write(dir.join("config/a.txt"), b"before-a").unwrap();
    fs::remove_file(dir.join("config/b.txt")).unwrap();
    write_atomic(
        &paths,
        &dir.join(".sporium/project-change.json"),
        &serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"id":point})).unwrap(),
    )
    .unwrap();
    transaction::recover(&paths, &dir).unwrap();
    transaction::recover(&paths, &dir).unwrap();
    assert_eq!(fs::read(dir.join("config/a.txt")).unwrap(), b"after-a");
    assert_eq!(fs::read(dir.join("config/b.txt")).unwrap(), b"after-b");
    assert!(!dir.join(".sporium/project-change.json").exists());
}

#[test]
fn corrupted_snapshot_and_changed_previews_refuse_before_any_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let (_, instance, manager, paths, dir) = fixture(temp.path());
    fs::write(dir.join("resourcepacks/a.zip"), b"before").unwrap();
    let plan = studio(&manager, &instance.id, "1.0");
    fs::write(dir.join("resourcepacks/a.zip"), b"changed").unwrap();
    assert!(manager.apply(&plan.token, true).is_err());
    assert!(manifest(&paths, &dir).unwrap().is_none());
    let point = manager
        .apply(&studio(&manager, &instance.id, "1.0").token, true)
        .unwrap();
    let snapshot = transaction::load(&paths, &dir, &point).unwrap();
    let blob = snapshot
        .after
        .get(".sporium/project.json")
        .unwrap()
        .as_ref()
        .unwrap();
    fs::write(transaction::data(&dir, &point, blob).unwrap(), b"corrupt").unwrap();
    assert!(transaction::restore(&paths, &dir, &instance, &point, true).is_err());
    assert_eq!(
        fs::read(dir.join("resourcepacks/a.zip")).unwrap(),
        b"changed"
    );
}

#[test]
fn transaction_paths_exclude_worlds_secrets_and_escape_paths() {
    for name in [
        "../escape",
        "saves/world/level.dat",
        "screenshots/image.png",
        ".sporium/skin.json",
        "C:/escape",
        "config/../../escape",
    ] {
        assert!(transaction::safe_path(name).is_err(), "{name}");
    }
    for name in [
        "config/settings.toml",
        "mods/a.jar",
        ".sporium/project.json",
    ] {
        assert!(transaction::safe_path(name).is_ok());
    }
}

#[test]
fn automatic_defaults_and_instance_overrides_are_independent_and_persistent() {
    use crate::content::{
        ContentManager,
        automatic::{AutomaticPolicy, InstanceUpdateMode, UpdateMode},
    };
    let temp = tempfile::tempdir().unwrap();
    let (library, instance, _, _, _) = fixture(temp.path());
    let manager = ContentManager::new(library).unwrap();
    let defaults = manager.automatic_policy().unwrap();
    assert_eq!(defaults.content, UpdateMode::Check);
    assert_eq!(defaults.project, UpdateMode::Check);
    let policy = AutomaticPolicy {
        schema_version: 1,
        content: UpdateMode::Install,
        project: UpdateMode::Off,
        instances: vec![InstanceUpdateMode {
            id: instance.id.clone(),
            content: Some(UpdateMode::Off),
            project: Some(UpdateMode::Check),
        }],
    };
    manager.save_automatic_policy(policy).unwrap();
    let saved = manager.automatic_policy().unwrap();
    assert_eq!(saved.mode(&instance.id, false), UpdateMode::Off);
    assert_eq!(saved.mode(&instance.id, true), UpdateMode::Check);
    assert_eq!(manager.automatic_check(&instance.id).unwrap().status, "off");
    let mut invalid = saved;
    invalid.instances.push(invalid.instances[0].clone());
    assert!(manager.save_automatic_policy(invalid).is_err());
    assert_eq!(manager.automatic_policy().unwrap().instances.len(), 1);
}

#[test]
fn local_project_update_automatic_consent_config_rollback_and_source_outage() {
    use crate::content::{
        ContentManager,
        automatic::{AutomaticPolicy, UpdateMode},
    };
    let temp = tempfile::tempdir().unwrap();
    let (library, instance, manager, paths, dir) = fixture(temp.path());
    fs::write(dir.join("config/managed.toml"), b"original").unwrap();
    let files = manager.studio_files(&instance.id).unwrap();
    let plan = manager
        .studio_plan(StudioRequest {
            id: instance.id.clone(),
            name: "Auto fixture".into(),
            version: "1".into(),
            forbid_external_mods: false,
            policies: files
                .into_iter()
                .map(|file| StudioFilePolicy {
                    path: file.path,
                    policy: FilePolicy::RequiredLocked,
                    group: None,
                })
                .collect(),
        })
        .unwrap();
    manager.apply(&plan.token, true).unwrap();
    let source = paths.root().join("launcher/source.json");
    let mut manifest = super::manifest(&paths, &dir).unwrap().unwrap();
    fs::write(&source, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let plan = manager.local_plan(&instance.id, &source, &[]).unwrap();
    manager.apply(&plan.token, true).unwrap();
    let content = ContentManager::new(library).unwrap();
    let replacement = paths.root().join("launcher/replacement");
    fs::write(&replacement, b"replacement").unwrap();
    let blob = transaction::digest(&replacement).unwrap();
    fs::copy(
        &replacement,
        paths
            .root()
            .join("launcher/project-sources")
            .join(format!("{}.bin", blob.sha512)),
    )
    .unwrap();
    manifest.version = "2".into();
    manifest.files[0].sha256 = blob.sha256;
    manifest.files[0].sha512 = blob.sha512;
    manifest.files[0].size = blob.size;
    fs::write(&source, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert_eq!(manager.check(&instance.id).unwrap().status, "available");
    content.automatic_cycle(&instance.id, &manager).unwrap();
    assert_eq!(
        fs::read(dir.join("config/managed.toml")).unwrap(),
        b"original"
    );
    content
        .save_automatic_policy(AutomaticPolicy {
            schema_version: 1,
            content: UpdateMode::Off,
            project: UpdateMode::Install,
            instances: vec![],
        })
        .unwrap();
    let report = content.automatic_cycle(&instance.id, &manager).unwrap();
    assert_eq!(report.status, "project_installed");
    assert_eq!(
        fs::read(dir.join("config/managed.toml")).unwrap(),
        b"replacement"
    );
    assert_eq!(super::manifest(&paths, &dir).unwrap().unwrap().version, "2");
    fs::remove_file(&source).unwrap();
    assert_eq!(manager.check(&instance.id).unwrap().status, "unavailable");
    assert!(validate_launch(&paths, &dir, &instance).is_ok());
}

#[test]
fn optional_groups_forbidden_policies_and_changed_source_are_reviewable() {
    let temp = tempfile::tempdir().unwrap();
    let (_, instance, manager, paths, dir) = fixture(temp.path());
    fs::write(dir.join("config/optional.txt"), b"optional").unwrap();
    let files = manager.studio_files(&instance.id).unwrap();
    let plan = manager
        .studio_plan(StudioRequest {
            id: instance.id.clone(),
            name: "Optional".into(),
            version: "1".into(),
            forbid_external_mods: false,
            policies: files
                .into_iter()
                .map(|file| StudioFilePolicy {
                    path: file.path,
                    policy: FilePolicy::Optional,
                    group: Some("Extras".into()),
                })
                .collect(),
        })
        .unwrap();
    let plan = manager.replan(&plan.token, &[]).unwrap();
    assert!(plan.selected_groups.is_empty());
    manager.apply(&plan.token, true).unwrap();
    assert!(!dir.join("config/optional.txt").exists());
    // A subsequently added forbidden file is removed only by an explicitly accepted plan.
    fs::write(dir.join("config/optional.txt"), b"optional").unwrap();
    let source = paths.root().join("launcher/project-source-test.json");
    let mut manifest = super::manifest(&paths, &dir).unwrap().unwrap();
    manifest.version = "2".into();
    manifest.files[0].policy = FilePolicy::UserForbidden;
    manifest.files[0].group = None;
    manifest.optional_groups.clear();
    fs::write(&source, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let plan = manager.local_plan(&instance.id, &source, &[]).unwrap();
    assert!(plan.warnings.iter().any(|w| w.starts_with("forbidden:")));
    fs::write(&source, b"changed manifest").unwrap();
    assert!(manager.apply(&plan.token, true).is_err());
    assert!(dir.join("config/optional.txt").exists());
    fs::write(&source, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let plan = manager.local_plan(&instance.id, &source, &[]).unwrap();
    manager.apply(&plan.token, true).unwrap();
    assert!(!dir.join("config/optional.txt").exists());
    assert!(guard_user_file(&paths, &dir, "config/optional.txt", true).is_err());
}
