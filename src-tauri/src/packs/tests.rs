use super::*;
use crate::storage::Database;

fn manifest() -> Manifest {
    Manifest {
        format_version: 1,
        game: "minecraft".into(),
        version_id: "1.0".into(),
        name: "Test pack".into(),
        summary: None,
        dependencies: BTreeMap::from([
            ("minecraft".into(), "1.21.1".into()),
            ("fabric-loader".into(), "0.16.14".into()),
        ]),
        files: vec![],
    }
}
fn setup() -> (tempfile::TempDir, PackManager) {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let manager = PackManager::new(Library::new(
        data.clone(),
        Database::new(data.join("launcher/sporium.sqlite3")),
    ));
    (root, manager)
}
fn zip(path: &Path, index: &Manifest, entries: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    zip.start_file(
        "modrinth.index.json",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(&serde_json::to_vec(index).unwrap()).unwrap();
    for (name, bytes) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
fn wait(manager: &PackManager) -> PackJob {
    let start = Instant::now();
    loop {
        let job = manager.snapshot().unwrap();
        if !manager.is_active() {
            return job;
        }
        assert!(start.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn unsafe_paths_and_secrets_are_rejected() {
    for name in [
        "../evil",
        "/root",
        "C:/evil",
        "mods/../evil",
        "mods/./a.jar",
        "mods//a.jar",
        "mods/CON.jar",
        "mods/a.jar:evil",
        "launcher_profiles.json",
        ".sporium/content.json",
        "accounts.json",
        "versions/1.jar",
        "automodpack/.private/automodpack-known-hosts.json",
        "data/.PRIVATE/credentials.json",
    ] {
        assert!(archive::allowed_path(name).is_err(), "{name}");
    }
    assert!(archive::allowed_path("config/example.json").is_ok());
}

#[test]
fn fabric_data_and_automodpack_settings_import_without_private_state() {
    let (root, manager) = setup();
    let pack = root.path().join("fresh-layout.mrpack");
    zip(
        &pack,
        &manifest(),
        &[
            (
                "overrides/data/fabricDefaultResourcePacks.dat",
                b"fabric-data",
            ),
            ("overrides/data/fabric_default_resource_packs.json", b"{}"),
            (
                "overrides/automodpack/automodpack-client.json",
                b"{\"playMusic\":false}",
            ),
            ("overrides/automodpack/automodpack-server.json", b"{}"),
            (
                "overrides/automodpack/.private/automodpack-known-hosts.json",
                b"private-fixture",
            ),
            (
                "client-overrides/data/fabric_default_resource_packs.json",
                b"{\"client\":true}",
            ),
        ],
    );
    let before = fs::read(&pack).unwrap();
    let preview = manager.preview(&pack).unwrap();
    assert!(
        preview
            .warnings
            .iter()
            .any(|warning| warning == "private_overrides_skipped:1")
    );
    manager
        .start(&preview.token, "Fresh layout", vec![])
        .unwrap();
    let job = wait(&manager);
    assert_eq!(job.phase, "completed", "{:?}", job.error);
    let game = manager
        .library
        .root()
        .join("instances")
        .join(job.instance_id.unwrap());
    assert_eq!(
        fs::read(game.join("data/fabricDefaultResourcePacks.dat")).unwrap(),
        b"fabric-data"
    );
    assert_eq!(
        fs::read(game.join("data/fabric_default_resource_packs.json")).unwrap(),
        b"{\"client\":true}"
    );
    assert_eq!(
        fs::read(game.join("automodpack/automodpack-client.json")).unwrap(),
        b"{\"playMusic\":false}"
    );
    assert!(!game.join("automodpack/.private").exists());
    assert_eq!(fs::read(&pack).unwrap(), before);
}
#[test]
fn dependency_schema_is_exact_and_unknown_loaders_fail() {
    let mut m = manifest();
    assert!(archive::metadata(&m).is_ok());
    m.dependencies.insert("forge".into(), "52.0.0".into());
    assert!(matches!(
        archive::metadata(&m),
        Err(CoreError::DependencyConflict)
    ));
    m.dependencies.remove("forge");
    m.dependencies
        .insert("quilt-loader".into(), "0.20.0".into());
    assert!(matches!(
        archive::metadata(&m),
        Err(CoreError::ContentUnsupported)
    ));
    m.dependencies.remove("quilt-loader");
    m.format_version = 2;
    assert!(matches!(
        archive::metadata(&m),
        Err(CoreError::SchemaTooNew)
    ));
}
#[test]
fn downloads_require_both_hashes_and_explicit_https_allowlist() {
    let mut m = manifest();
    let mut file = PackFile {
        path: "mods/a.jar".into(),
        hashes: BTreeMap::from([
            ("sha512".into(), "a".repeat(128)),
            ("sha1".into(), "b".repeat(40)),
        ]),
        downloads: vec!["https://cdn.modrinth.com/data/id/versions/v/a.jar".into()],
        file_size: 4,
        env: None,
    };
    m.files.push(file.clone());
    assert!(archive::metadata(&m).is_ok());
    for url in [
        "http://cdn.modrinth.com/a.jar",
        "https://cdn.modrinth.com.evil/a.jar",
        "https://u:p@cdn.modrinth.com/a.jar",
        "https://cdn.modrinth.com:8443/a.jar",
        "https://localhost/a.jar",
    ] {
        m.files[0].downloads = vec![url.into()];
        assert!(archive::metadata(&m).is_err());
    }
    file.hashes.remove("sha1");
    m.files = vec![file];
    assert!(archive::metadata(&m).is_err());
}
#[test]
fn archive_preflight_rejects_traversal_case_aliases_and_file_parents() {
    let (root, manager) = setup();
    for entries in [
        vec![("overrides/../evil", b"x".as_slice())],
        vec![
            ("overrides/config/A.txt", b"x".as_slice()),
            ("overrides/config/a.txt", b"x".as_slice()),
        ],
        vec![
            ("overrides/config/a", b"x".as_slice()),
            ("overrides/config/a/b", b"x".as_slice()),
        ],
        vec![("overrides/accounts.json", b"secret".as_slice())],
    ] {
        let pack = root.path().join("bad.mrpack");
        zip(&pack, &manifest(), &entries);
        assert!(manager.preview(&pack).is_err());
        assert!(manager.library.snapshot().unwrap().instances.is_empty());
    }
}
#[test]
fn overrides_client_layer_wins_and_server_layer_is_not_installed() {
    let (root, manager) = setup();
    let pack = root.path().join("test.mrpack");
    zip(
        &pack,
        &manifest(),
        &[
            ("overrides/config/a.txt", b"shared"),
            ("client-overrides/config/a.txt", b"client"),
            ("server-overrides/config/a.txt", b"server"),
        ],
    );
    let preview = manager.preview(&pack).unwrap();
    assert!(
        preview
            .warnings
            .iter()
            .any(|w| w == "server_overrides_skipped")
    );
    manager
        .start(&preview.token, "Client pack", vec![])
        .unwrap();
    let job = wait(&manager);
    assert_eq!(job.phase, "completed", "{:?}", job.error);
    let id = job.instance_id.unwrap();
    assert_eq!(
        fs::read(
            manager
                .library
                .root()
                .join("instances")
                .join(id)
                .join("config/a.txt")
        )
        .unwrap(),
        b"client"
    );
    let snapshot = manager.library.snapshot().unwrap();
    assert_eq!(
        snapshot.instances[0].loader_version.as_deref(),
        Some("0.16.14")
    );
}
#[test]
fn preview_freezes_archive_and_dismiss_cannot_publish() {
    let (root, manager) = setup();
    let pack = root.path().join("test.mrpack");
    zip(&pack, &manifest(), &[("overrides/options.txt", b"old")]);
    let preview = manager.preview(&pack).unwrap();
    zip(&pack, &manifest(), &[("overrides/options.txt", b"new")]);
    manager.start(&preview.token, "Frozen", vec![]).unwrap();
    let job = wait(&manager);
    assert_eq!(job.phase, "completed");
    assert_eq!(
        fs::read(
            manager
                .library
                .root()
                .join("instances")
                .join(job.instance_id.unwrap())
                .join("options.txt")
        )
        .unwrap(),
        b"old"
    );
    let preview = manager.preview(&pack).unwrap();
    manager.dismiss(&preview.token);
    assert!(matches!(
        manager.start(&preview.token, "Cancelled", vec![]),
        Err(CoreError::NotFound)
    ));
}
#[test]
fn client_optional_selection_and_server_only_files_are_explicit() {
    let (root, manager) = setup();
    let pack = root.path().join("optional.mrpack");
    let mut m = manifest();
    for (name, env) in [
        ("mods/optional.jar", "optional"),
        ("mods/server.jar", "unsupported"),
    ] {
        m.files.push(PackFile {
            path: name.into(),
            hashes: BTreeMap::from([
                ("sha512".into(), "a".repeat(128)),
                ("sha1".into(), "b".repeat(40)),
            ]),
            downloads: vec!["https://cdn.modrinth.com/file.jar".into()],
            file_size: 12,
            env: Some(Environment {
                client: env.into(),
                server: "required".into(),
            }),
        });
    }
    zip(&pack, &m, &[]);
    let p = manager.preview(&pack).unwrap();
    assert_eq!(p.optional_files, vec!["mods/optional.jar"]);
    assert_eq!(p.download_bytes, 0);
    assert!(
        manager
            .start(&p.token, "Bad", vec!["mods/server.jar".into()])
            .is_err()
    );
    manager.start(&p.token, "No optional", vec![]).unwrap();
    assert_eq!(wait(&manager).phase, "completed");
}
#[test]
fn sporium_round_trip_preserves_settings_worlds_and_exact_loader_in_new_instance() {
    let (root, manager) = setup();
    let pack = root.path().join("roundtrip.mrpack");
    zip(
        &pack,
        &manifest(),
        &[
            ("overrides/options.txt", b"fullscreen:false"),
            ("overrides/config/settings.json", b"{}"),
            ("overrides/saves/World/level.dat", b"world-bytes"),
        ],
    );
    let p = manager.preview(&pack).unwrap();
    manager.start(&p.token, "Original", vec![]).unwrap();
    let old = wait(&manager).instance_id.unwrap();
    let output = root.path().join("export.sporium");
    let report = manager
        .export(
            PackExportRequest {
                id: old.clone(),
                include_worlds: true,
                include_local: false,
            },
            &output,
        )
        .unwrap();
    assert_eq!(report.embedded_files, 3);
    let p = manager.preview(&output).unwrap();
    manager.start(&p.token, "Copy", vec![]).unwrap();
    let new = wait(&manager).instance_id.unwrap();
    assert_ne!(old, new);
    for id in [old, new] {
        assert_eq!(
            fs::read(
                manager
                    .library
                    .root()
                    .join("instances")
                    .join(id)
                    .join("saves/World/level.dat")
            )
            .unwrap(),
            b"world-bytes"
        );
    }
    assert_eq!(manager.library.snapshot().unwrap().instances.len(), 2);
}
#[test]
fn external_prism_import_is_non_destructive_and_excludes_accounts_and_hooks() {
    let (root, manager) = setup();
    let source = root.path().join("Prism");
    fs::create_dir_all(source.join(".minecraft/saves/World")).unwrap();
    fs::write(
        source.join("instance.cfg"),
        "name=Prism copy\nPreLaunchCommand=evil\n",
    )
    .unwrap();
    fs::write(source.join("mmc-pack.json"),r#"{"formatVersion":1,"components":[{"uid":"net.minecraft","version":"1.21.1"},{"uid":"net.fabricmc.fabric-loader","version":"0.16.14"}]}"#).unwrap();
    fs::write(source.join(".minecraft/accounts.json"), "SECRET").unwrap();
    fs::write(source.join(".minecraft/saves/World/level.dat"), "world").unwrap();
    let c = manager.scan(&source).unwrap();
    let p = manager.external_preview(&c[0].key).unwrap();
    assert!(
        p.warnings
            .iter()
            .any(|w| w == "source_item_skipped:accounts.json")
    );
    manager.start(&p.token, "Prism imported", vec![]).unwrap();
    let j = wait(&manager);
    assert_eq!(j.phase, "completed");
    assert_eq!(
        fs::read(source.join(".minecraft/accounts.json")).unwrap(),
        b"SECRET"
    );
    assert!(
        !manager
            .library
            .root()
            .join("instances")
            .join(j.instance_id.unwrap())
            .join("accounts.json")
            .exists()
    );
}
#[test]
fn unsupported_directory_and_generic_zip_are_never_guessed() {
    let (root, manager) = setup();
    let pack = root.path().join("test.zip");
    zip(&pack, &manifest(), &[]);
    assert!(manager.preview(&pack).is_err());
    assert!(manager.scan(root.path()).is_err());
}
#[test]
fn abandoned_private_stages_are_pruned_but_active_previews_and_unknown_folders_remain() {
    let (_root, manager) = setup();
    let (paths, active) = manager.stage().unwrap();
    let active_path = active.path().to_path_buf();
    let (_paths, abandoned) = manager.stage().unwrap();
    let Stage { tree, _lease } = abandoned;
    let orphan = tree.keep();
    drop(_lease);
    let unknown = paths.root().join("launcher/pack-staging/pack-unknown");
    paths.mkdir(&unknown).unwrap();
    fs::write(unknown.join("keep.txt"), b"keep").unwrap();
    let _fresh = manager.stage().unwrap();
    assert!(!orphan.exists());
    assert!(active_path.exists());
    assert!(unknown.join("keep.txt").exists());
}
#[test]
fn external_disabled_mod_names_map_without_changing_the_source_and_collisions_fail() {
    let (root, manager) = setup();
    let source = root.path().join("disabled");
    fs::create_dir_all(source.join("mods")).unwrap();
    fs::create_dir_all(source.join("disabledmods")).unwrap();
    fs::write(source.join("mods/one.jar.disabled"), b"one").unwrap();
    fs::write(source.join("disabledmods/two.jar"), b"two").unwrap();
    let (paths, tree) = manager.stage().unwrap();
    let (copied, _) = external::copy(&paths, &source, tree.path()).unwrap();
    assert!(copied.contains_key("mods_disabled/one.jar"));
    assert!(copied.contains_key("mods_disabled/two.jar"));
    assert!(source.join("mods/one.jar.disabled").exists());
    fs::write(source.join("disabledmods/one.jar"), b"collision").unwrap();
    assert!(external::files(&source, true).is_err());
}

#[test]
fn download_cache_still_checks_sha1_and_never_publishes_corrupt_payload() {
    use sha1::Digest;
    let (root, manager) = setup();
    let payload = b"verified settings";
    let sha512 = crate::game::network::hex(&sha2::Sha512::digest(payload));
    let paths = Paths::new(manager.library.root()).unwrap();
    let cache = paths
        .root()
        .join("shared/cache/modrinth/artifacts")
        .join(format!("{sha512}.bin"));
    paths.mkdir(cache.parent().unwrap()).unwrap();
    fs::write(cache, payload).unwrap();
    let mut m = manifest();
    m.files.push(PackFile {
        path: "config/remote.txt".into(),
        hashes: BTreeMap::from([("sha512".into(), sha512), ("sha1".into(), "0".repeat(40))]),
        downloads: vec!["https://cdn.modrinth.com/example.txt".into()],
        file_size: payload.len() as u64,
        env: None,
    });
    let pack = root.path().join("bad-hash.mrpack");
    zip(&pack, &m, &[]);
    let p = manager.preview(&pack).unwrap();
    manager.start(&p.token, "Bad hash", vec![]).unwrap();
    let job = wait(&manager);
    assert_eq!(job.phase, "failed");
    assert_eq!(job.error.unwrap().code, crate::error::ErrorCode::Integrity);
    assert!(manager.library.snapshot().unwrap().instances.is_empty());
    m.files[0].hashes.insert(
        "sha1".into(),
        crate::game::network::hex(&sha1::Sha1::digest(payload)),
    );
    zip(&pack, &m, &[]);
    let p = manager.preview(&pack).unwrap();
    manager.start(&p.token, "Good hash", vec![]).unwrap();
    let job = wait(&manager);
    assert_eq!(job.phase, "completed");
    assert_eq!(
        fs::read(
            manager
                .library
                .root()
                .join("instances")
                .join(job.instance_id.unwrap())
                .join("config/remote.txt")
        )
        .unwrap(),
        payload
    );
}
#[test]
fn cancellation_and_changed_preview_bytes_leave_no_instance() {
    let (root, manager) = setup();
    let pack = root.path().join("cancel.mrpack");
    zip(&pack, &manifest(), &[("overrides/options.txt", b"value")]);
    let p = manager.preview(&pack).unwrap();
    let plan = manager
        .state
        .lock()
        .unwrap()
        .plans
        .remove(&p.token)
        .unwrap();
    let paths = Paths::new(manager.library.root()).unwrap();
    let (request, version) = archive::metadata(&plan.manifest).unwrap();
    manager.cancel();
    assert!(matches!(
        manager.install(&paths, &plan, request.clone(), version.clone(), &[], true),
        Err(CoreError::Cancelled)
    ));
    manager.cancel.store(false, Ordering::SeqCst);
    fs::write(plan.tree.path().join("game/options.txt"), b"tampered").unwrap();
    assert!(matches!(
        manager.install(&paths, &plan, request, version, &[], true),
        Err(CoreError::SourceChanged)
    ));
    assert!(manager.library.snapshot().unwrap().instances.is_empty());
}
#[test]
fn sporium_checksum_tampering_and_new_schema_are_rejected() {
    let (root, manager) = setup();
    let target = root.path().join("bad.sporium");
    for schema in [1, 2] {
        let pack = SporiumPack {
            schema_version: schema,
            manifest: manifest(),
            overrides: BTreeMap::from([(
                "options.txt".into(),
                EmbeddedFile {
                    sha512: "0".repeat(128),
                    size: 5,
                },
            )]),
        };
        let mut zip = zip::ZipWriter::new(File::create(&target).unwrap());
        zip.start_file(
            "sporium.index.json",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&serde_json::to_vec(&pack).unwrap()).unwrap();
        zip.start_file(
            "overrides/options.txt",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"value").unwrap();
        zip.finish().unwrap();
        assert!(manager.preview(&target).is_err());
    }
}
#[test]
fn export_excludes_local_binaries_worlds_secrets_and_never_overwrites() {
    let (root, manager) = setup();
    let request = CreateInstance {
        name: "Export scope".into(),
        minecraft_version: "1.21.1".into(),
        loader: crate::instances::model::Loader::Vanilla,
        collection_id: None,
    };
    let id = manager.library.create(request).unwrap().affected_id;
    let instance = manager.library.root().join("instances").join(&id);
    fs::write(instance.join("mods/local.jar"), b"local").unwrap();
    fs::write(instance.join("config/value.txt"), b"setting").unwrap();
    fs::create_dir_all(instance.join("saves/world")).unwrap();
    fs::write(instance.join("saves/world/level.dat"), b"world").unwrap();
    fs::write(instance.join("accounts.json"), b"secret").unwrap();
    let target = root.path().join("scope.sporium");
    let req = PackExportRequest {
        id: id.clone(),
        include_local: false,
        include_worlds: false,
    };
    let report = manager.export(req.clone(), &target).unwrap();
    assert_eq!(report.embedded_files, 1);
    assert!(report.omitted.iter().any(|s| s == "mods/local.jar"));
    let before = archive::digest(&target).unwrap();
    assert!(manager.export(req, &target).is_err());
    assert_eq!(before.sha512, archive::digest(&target).unwrap().sha512);
    let p = manager.preview(&target).unwrap();
    manager.start(&p.token, "Clean copy", vec![]).unwrap();
    let job = wait(&manager);
    assert_eq!(job.phase, "completed");
    let imported = manager
        .library
        .root()
        .join("instances")
        .join(job.instance_id.unwrap());
    assert!(!imported.join("accounts.json").exists());
    assert!(!imported.join("saves/world/level.dat").exists());
    assert!(!imported.join("mods/local.jar").exists());
}
#[test]
fn external_atlauncher_and_official_profile_metadata_are_mapped_conservatively() {
    let (root, manager) = setup();
    let at = root.path().join("AT");
    fs::create_dir_all(&at).unwrap();
    fs::write(at.join("instance.json"),r#"{"id":"fabric-version","inheritsFrom":"1.21.1","launcher":{"name":"AT Fabric","loaderVersion":{"type":"Fabric","version":"0.16.14"}}}"#).unwrap();
    fs::write(at.join("options.txt"), "option").unwrap();
    let views = manager.scan(&at).unwrap();
    assert_eq!(views[0].loader, crate::instances::model::Loader::Fabric);
    let p = manager.external_preview(&views[0].key).unwrap();
    assert_eq!(p.minecraft, "1.21.1");
    manager.dismiss(&p.token);
    fs::write(at.join("instance.json"), "{}").unwrap();
    assert!(matches!(
        manager.external_preview(&views[0].key),
        Err(CoreError::SourceChanged)
    ));
    let official = root.path().join("minecraft");
    fs::create_dir_all(official.join("versions/1.21.1")).unwrap();
    fs::write(
        official.join("versions/1.21.1/1.21.1.json"),
        r#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main"}"#,
    )
    .unwrap();
    fs::write(official.join("launcher_profiles.json"),r#"{"profiles":{"test":{"name":"Official","lastVersionId":"1.21.1"},"unresolved":{"lastVersionId":"latest-release"}}}"#).unwrap();
    let views = manager.scan(&official).unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].loader, crate::instances::model::Loader::Vanilla);
    let p = manager.external_preview(&views[0].key).unwrap();
    assert!(
        p.warnings
            .iter()
            .any(|w| w == "source_item_skipped:launcher_profiles.json")
    );
}
#[test]
fn modrinth_sqlite_is_read_only_and_active_wal_is_refused() {
    let (root, manager) = setup();
    let source = root.path().join("Modrinth");
    fs::create_dir_all(source.join("profiles/one")).unwrap();
    fs::write(source.join("profiles/one/options.txt"), "setting").unwrap();
    let db = source.join("app.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TABLE instances(id TEXT,path TEXT,name TEXT,applied_content_set_id TEXT,install_stage TEXT); CREATE TABLE instance_content_sets(id TEXT,instance_id TEXT,game_version TEXT,loader TEXT,loader_version TEXT); INSERT INTO instances VALUES('one','one','Modrinth copy','content','installed'); INSERT INTO instance_content_sets VALUES('content','one','1.21.1','fabric','0.16.14'); CREATE TABLE minecraft_users(token TEXT); INSERT INTO minecraft_users VALUES('SECRET');").unwrap();
    drop(conn);
    let before = archive::digest(&db).unwrap();
    let c = manager.scan(&source).unwrap();
    assert_eq!(c[0].source, "Modrinth App");
    let p = manager.external_preview(&c[0].key).unwrap();
    manager.start(&p.token, "Modrinth copy", vec![]).unwrap();
    assert_eq!(wait(&manager).phase, "completed");
    assert_eq!(archive::digest(&db).unwrap().sha512, before.sha512);
    assert!(!source.join("app.db-shm").exists());
    fs::write(source.join("app.db-wal"), b"uncheckpointed").unwrap();
    assert!(matches!(
        manager.scan(&source),
        Err(CoreError::SourceChanged)
    ));
}
#[cfg(windows)]
#[test]
fn external_reparse_point_is_rejected() {
    use std::os::windows::process::CommandExt;
    let (root, manager) = setup();
    let outside = root.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let link = root.path().join("linked");
    let status = std::process::Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .creation_flags(0x08000000)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(matches!(manager.scan(&link), Err(CoreError::UnsafePath)));
    fs::remove_dir(link).unwrap();
    assert!(outside.exists());
}
