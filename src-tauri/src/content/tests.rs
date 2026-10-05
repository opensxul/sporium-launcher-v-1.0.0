use super::*;
use crate::{
    game::network::hex,
    instances::model::{CreateInstance, Loader, now},
    storage::Database,
};
use sha1::Digest;

#[test]
fn fabric_multiline_descriptions_match_loader_without_accepting_broken_structure() {
    let temp = tempfile::tempdir().unwrap();
    let (_, instance) = library(temp.path());
    let jar = temp.path().join("multiline.jar");
    local_jar(
        &jar,
        &[(
            "fabric.mod.json",
            r#"{"schemaVersion":1,"id":"multiline","version":"1.0","name":"Multiline","description":"First line
Second line with \"quotes\" and a \\ slash","depends":{"minecraft":"1.21.1"}}"#,
        )],
    );
    let (name, _, metadata) = local::inspect(&jar, &instance).unwrap();
    assert_eq!(name, "Multiline");
    assert_eq!(metadata.mod_ids, vec!["multiline"]);
    local_jar(
        &jar,
        &[(
            "fabric.mod.json",
            "{\"id\":\"broken\",\"description\":\"line\nline\"",
        )],
    );
    assert!(local::inspect(&jar, &instance).is_err());
}

struct Fixture {
    projects: HashMap<String, ContentProject>,
    versions: Vec<ContentVersion>,
}
impl ContentProvider for Fixture {
    fn version_from_hash(&self, sha512: &str) -> Result<Option<ContentVersion>, CoreError> {
        Ok(self
            .versions
            .iter()
            .find(|v| v.files.iter().any(|f| f.hashes.sha512 == sha512))
            .cloned())
    }
    fn search(&self, _: &CatalogQuery) -> Result<CatalogPage, CoreError> {
        unreachable!()
    }
    fn tags(&self) -> Result<ContentTags, CoreError> {
        unreachable!()
    }
    fn project(&self, id: &str) -> Result<ContentProject, CoreError> {
        self.projects.get(id).cloned().ok_or(CoreError::NotFound)
    }
    fn version(&self, id: &str) -> Result<ContentVersion, CoreError> {
        self.versions
            .iter()
            .find(|v| v.id == id)
            .cloned()
            .ok_or(CoreError::NotFound)
    }
    fn versions(&self, id: &str, _: &str) -> Result<Vec<ContentVersion>, CoreError> {
        Ok(self
            .versions
            .iter()
            .filter(|v| v.project_id == id)
            .cloned()
            .collect())
    }
}
fn version(project: &str, id: &str, dependency: Option<&str>) -> ContentVersion {
    let bytes = b"verified test content";
    ContentVersion {
        id: id.into(),
        project_id: project.into(),
        name: id.into(),
        version_number: "1.0".into(),
        version_type: "release".into(),
        date_published: "2026-09-28".into(),
        status: "listed".into(),
        game_versions: vec!["1.21.1".into()],
        loaders: vec!["fabric".into()],
        environment: "client_only".into(),
        dependencies: dependency
            .map(|id| {
                vec![ContentDependency {
                    version_id: None,
                    project_id: Some(id.into()),
                    file_name: None,
                    dependency_type: "required".into(),
                }]
            })
            .unwrap_or_default(),
        files: vec![ContentFile {
            id: None,
            filename: format!("{project}.jar"),
            url: format!("https://cdn.modrinth.com/data/{project}/{id}.jar"),
            hashes: ContentHashes {
                sha1: hex(&sha1::Sha1::digest(bytes)),
                sha512: hex(&sha2::Sha512::digest(bytes)),
            },
            size: bytes.len() as u64,
            primary: true,
            file_type: None,
        }],
    }
}
fn fixture() -> Fixture {
    let projects = ["parent", "dependency"]
        .into_iter()
        .map(|id| {
            (
                id.into(),
                ContentProject {
                    icon_url: None,
                    id: id.into(),
                    title: id.into(),
                    description: String::new(),
                    body: String::new(),
                    project_type: "mod".into(),
                    license: ContentLicense {
                        id: "MIT".into(),
                        name: "MIT".into(),
                    },
                    client_side: "required".into(),
                    server_side: "optional".into(),
                    environment: vec!["client_only".into()],
                },
            )
        })
        .collect();
    Fixture {
        projects,
        versions: vec![
            version("parent", "parentVersion", Some("dependency")),
            version("dependency", "depVersion", None),
        ],
    }
}
fn library(root: &std::path::Path) -> (Library, Instance) {
    let library = Library::new(
        root.into(),
        Database::new(root.join("launcher/sporium.sqlite3")),
    );
    library
        .create(CreateInstance {
            name: "content fixture".into(),
            minecraft_version: "1.21.1".into(),
            loader: Loader::Fabric,
            collection_id: None,
        })
        .unwrap();
    let instance = library.snapshot().unwrap().instances.remove(0);
    (library, instance)
}
fn request(instance: &Instance) -> ContentRequest {
    ContentRequest {
        instance_id: instance.id.clone(),
        project_id: "parent".into(),
        version_id: "parentVersion".into(),
    }
}
fn local_jar(path: &std::path::Path, entries: &[(&str, &str)]) {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, body) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

fn world_request(
    instance: &Instance,
    source: &std::path::Path,
    kind: &str,
    world: Option<&str>,
) -> WorldArchiveRequest {
    WorldArchiveRequest {
        instance_id: instance.id.clone(),
        source: source.to_string_lossy().into(),
        kind: kind.into(),
        world: world.map(String::from),
        title: "Imported test world".into(),
    }
}
fn datapack_zip(path: &std::path::Path) {
    local_jar(
        path,
        &[
            (
                "pack.mcmeta",
                r#"{"pack":{"pack_format":48,"description":"test"}}"#,
            ),
            ("data/test/function/load.mcfunction", "say test"),
        ],
    );
}
fn create_world(directory: &std::path::Path, id: &str) {
    std::fs::create_dir_all(directory.join("saves").join(id)).unwrap();
    std::fs::write(
        directory.join("saves").join(id).join("level.dat"),
        b"preserve world",
    )
    .unwrap();
}
#[test]
fn map_import_is_frozen_explicit_new_and_persistent_without_overwriting_worlds() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    create_world(&directory, "existing");
    let source = temp.path().join("map.zip");
    local_jar(
        &source,
        &[
            ("Wrapper/level.dat", "new world"),
            ("Wrapper/region/r.0.0.mca", "region"),
        ],
    );
    let plan = manager
        .world_plan(world_request(&instance, &source, "map", None))
        .unwrap();
    assert!(!directory.join("saves").join(&plan.world).exists());
    assert!(matches!(
        manager.world_finish(&plan.token, false, false),
        Err(CoreError::ContentIncompatible)
    ));
    manager.world_finish(&plan.token, false, true).unwrap();
    assert_eq!(manager.worlds(&instance.id).unwrap().len(), 1);
    assert!(manager.history(&instance.id).unwrap().is_empty());
    let plan = manager
        .world_plan(world_request(&instance, &source, "map", None))
        .unwrap();
    std::fs::write(&source, b"changed source after preview").unwrap();
    manager.world_finish(&plan.token, true, false).unwrap();
    assert_eq!(
        std::fs::read(directory.join("saves").join(&plan.world).join("level.dat")).unwrap(),
        b"new world"
    );
    assert_eq!(
        std::fs::read(directory.join("saves/existing/level.dat")).unwrap(),
        b"preserve world"
    );
    let restarted = ContentManager::new(manager.library.clone()).unwrap();
    let worlds = restarted.worlds(&instance.id).unwrap();
    assert_eq!(worlds.len(), 2);
    assert_eq!(
        worlds
            .iter()
            .find(|w| w.id == plan.world)
            .unwrap()
            .imported
            .as_ref()
            .unwrap()
            .source,
        "map.zip"
    );
    assert_eq!(
        restarted
            .history(&instance.id)
            .unwrap()
            .iter()
            .filter(|e| e.action == "import_world")
            .count(),
        1
    );
}
#[test]
fn world_archive_refuses_unsafe_ambiguous_corrupt_and_linked_payloads_before_publication() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    let source = temp.path().join("bad.zip");
    for entries in [
        vec![("../outside", "bad"), ("level.dat", "world")],
        vec![("one/level.dat", "world"), ("two/level.dat", "world")],
        vec![("world/level.dat", "world"), ("readme.txt", "ambiguous")],
        vec![("level.dat", "world"), ("LEVEL.DAT", "collision")],
        vec![("level.dat", "world"), ("aux.txt", "device")],
        vec![("nested/world/level.dat", "world")],
    ] {
        local_jar(&source, &entries);
        assert!(
            manager
                .world_plan(world_request(&instance, &source, "map", None))
                .is_err()
        );
        assert!(manager.worlds(&instance.id).unwrap().is_empty());
    }
    std::fs::write(&source, b"corrupt zip").unwrap();
    assert!(
        manager
            .world_plan(world_request(&instance, &source, "map", None))
            .is_err()
    );
    assert!(!directory.join("outside").exists());
}
#[test]
fn local_datapack_requires_real_explicit_world_and_preserves_other_worlds_and_source() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    create_world(&directory, "one");
    create_world(&directory, "two");
    let source = temp.path().join("data.zip");
    datapack_zip(&source);
    let bytes = std::fs::read(&source).unwrap();
    for target in [None, Some("../two"), Some("missing")] {
        assert!(
            manager
                .world_plan(world_request(&instance, &source, "datapack", target))
                .is_err()
        );
    }
    let plan = manager
        .world_plan(world_request(&instance, &source, "datapack", Some("one")))
        .unwrap();
    manager.world_finish(&plan.token, true, false).unwrap();
    assert_eq!(
        std::fs::read(directory.join("saves/one/datapacks/data.zip")).unwrap(),
        bytes
    );
    assert!(!directory.join("saves/two/datapacks").exists());
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    assert!(
        manager
            .world_plan(world_request(&instance, &source, "datapack", Some("one")))
            .is_err()
    );
    let rows = manager.installed(&instance.id).unwrap();
    assert_eq!(rows[0].record.kind, "datapack");
    manager
        .change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Delete,
            files: selection(&rows),
        })
        .unwrap();
    assert!(manager.installed(&instance.id).unwrap().is_empty());
    assert!(directory.join("saves/one/level.dat").exists());
    assert!(directory.join("saves/two/level.dat").exists());
}
#[test]
fn datapack_rejects_wrappers_non_data_archives_and_changed_world_after_preview() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    create_world(&directory, "one");
    let source = temp.path().join("data.zip");
    for entries in [
        vec![
            ("wrapper/pack.mcmeta", "{}"),
            ("wrapper/data/x/test", "test"),
        ],
        vec![
            ("pack.mcmeta", r#"{"pack":{"pack_format":48}}"#),
            ("assets/test/texture", "resourcepack"),
        ],
        vec![
            ("pack.mcmeta", r#"{"pack":{"pack_format":0}}"#),
            ("data/test/test", "bad"),
        ],
    ] {
        local_jar(&source, &entries);
        assert!(
            manager
                .world_plan(world_request(&instance, &source, "datapack", Some("one")))
                .is_err()
        );
    }
    datapack_zip(&source);
    let plan = manager
        .world_plan(world_request(&instance, &source, "datapack", Some("one")))
        .unwrap();
    std::fs::write(directory.join("saves/one/level.dat"), b"world changed").unwrap();
    assert!(matches!(
        manager.world_finish(&plan.token, true, false),
        Err(CoreError::RecordConflict)
    ));
    assert!(!directory.join("saves/one/datapacks/data.zip").exists());
    manager.world_finish(&plan.token, false, true).unwrap();
}
#[test]
fn missing_world_does_not_hide_other_managed_content_and_datapack_journal_cannot_recreate_it() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    create_world(&directory, "one");
    let source = temp.path().join("data.zip");
    datapack_zip(&source);
    let plan = manager
        .world_plan(world_request(&instance, &source, "datapack", Some("one")))
        .unwrap();
    manager.world_finish(&plan.token, true, false).unwrap();
    std::fs::remove_dir_all(directory.join("saves/one")).unwrap();
    assert_eq!(
        manager.installed(&instance.id).unwrap()[0].status,
        "missing"
    );
}
#[test]
fn map_tree_or_archive_tampering_refuses_commit_with_no_partial_world() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    let source = temp.path().join("map.zip");
    local_jar(&source, &[("level.dat", "world"), ("region/file", "data")]);
    for corrupt in ["world/region/file", "archive.zip", "world/extra.txt"] {
        let plan = manager
            .world_plan(world_request(&instance, &source, "map", None))
            .unwrap();
        std::fs::write(
            directory
                .join(".sporium/world-staging")
                .join(&plan.token)
                .join(corrupt),
            b"changed",
        )
        .unwrap();
        assert!(matches!(
            manager.world_finish(&plan.token, true, false),
            Err(CoreError::Integrity)
        ));
        assert!(!directory.join("saves").join(&plan.world).exists());
        manager.world_finish(&plan.token, false, true).unwrap();
    }
}
#[test]
fn modrinth_datapacks_require_zip_platform_game_and_world_and_keep_required_dependency_scope() {
    let temp = tempfile::tempdir().unwrap();
    let (mut manager, instance, directory) = adoption_fixture(temp.path());
    create_world(&directory, "one");
    create_world(&directory, "two");
    let mut provider = fixture();
    for version in &mut provider.versions {
        version.loaders = vec!["datapack".into()];
        version.files[0].filename = format!("{}.zip", version.project_id);
        version.files[0].url = format!(
            "https://cdn.modrinth.com/data/{}/data.zip",
            version.project_id
        );
        version.environment = "server_only".into();
    }
    for project in provider.projects.values_mut() {
        project.client_side = "unsupported".into();
    }
    manager.provider = Arc::new(provider);
    let request = || WorldProjectRequest {
        instance_id: instance.id.clone(),
        project_id: "parent".into(),
        version_id: "parentVersion".into(),
        world: "one".into(),
    };
    let plan = manager.world_project_plan(request()).unwrap();
    assert_eq!(plan.files.len(), 2);
    assert!(
        plan.files
            .iter()
            .all(|r| r.kind == "datapack" && r.directory == "saves/one/datapacks")
    );
    assert!(manager.plan(super::tests::request(&instance)).is_err());
    let mut wrong = request();
    wrong.world = "../two".into();
    assert!(manager.world_project_plan(wrong).is_err());
    std::fs::write(directory.join("saves/one/level.dat"), b"different world").unwrap();
    assert!(matches!(
        manager.start(&plan.token),
        Err(CoreError::RecordConflict)
    ));
    assert!(!directory.join("saves/two/datapacks").exists());
}
#[test]
fn map_journal_recovers_before_and_after_publish_once_without_touching_existing_world() {
    for published in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let (manager, instance, directory) = adoption_fixture(temp.path());
        create_world(&directory, "existing");
        let source = temp.path().join("map.zip");
        local_jar(
            &source,
            &[("level.dat", "world"), ("region/file", "region")],
        );
        let plan = manager
            .world_plan(world_request(&instance, &source, "map", None))
            .unwrap();
        let staged = directory.join(".sporium/world-staging").join(&plan.token);
        let files: Vec<_> = ["level.dat", "region/file"].into_iter().map(|name| {
            let artifact = worlds::artifact(&staged.join("world").join(name), "file").unwrap();
            serde_json::json!({"name":name,"sha512":artifact.hashes.sha512,"size":artifact.size})
        }).collect();
        let journal = serde_json::json!({"schema":1,"token":plan.token,"imported":{
            "id":plan.world,"title":plan.title,"source":plan.source,
            "sha512":worlds::artifact(&source,"map.zip").unwrap().hashes.sha512,"importedAt":now()
        },"files":files});
        std::fs::write(
            directory.join(".sporium/world-import.json"),
            serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        if published {
            std::fs::rename(
                staged.join("world"),
                directory.join("saves").join(&plan.world),
            )
            .unwrap();
        }
        let paths = Paths::new(temp.path()).unwrap();
        install::recover(&paths, &directory).unwrap();
        install::recover(&paths, &directory).unwrap();
        assert_eq!(manager.worlds(&instance.id).unwrap().len(), 2);
        assert_eq!(manager.history(&instance.id).unwrap().len(), 1);
        assert_eq!(
            std::fs::read(directory.join("saves/existing/level.dat")).unwrap(),
            b"preserve world"
        );
        assert!(!staged.exists());
        assert!(!directory.join(".sporium/world-import.json").exists());
    }
}
#[test]
fn datapack_updates_preserve_all_world_destinations_and_project_pins() {
    let temp = tempfile::tempdir().unwrap();
    let (_, instance) = library(temp.path());
    let mut provider = fixture();
    provider.versions[0].dependencies.clear();
    for version in &mut provider.versions {
        version.loaders = vec!["datapack".into()];
        version.files[0].filename = format!("{}.zip", version.project_id);
    }
    let mut resource = provider.versions[0].files[0].clone();
    resource.filename = "attached-resource.zip".into();
    resource.primary = false;
    resource.file_type = Some("required-resource-pack".into());
    provider.versions[0].files.push(resource);
    let mut installed = vec![];
    for world in ["one", "two"] {
        for record in worlds::selected(
            &provider.projects["parent"],
            &provider.versions[0],
            world,
            false,
        )
        .unwrap()
        {
            if !installed.iter().any(|old: &ContentRecord| {
                old.directory == record.directory && old.file.filename == record.file.filename
            }) {
                installed.push(record);
            }
        }
    }
    let mut newer = provider.versions[0].clone();
    newer.id = "parentNew".into();
    newer.date_published = "2026-10-01".into();
    newer.dependencies = vec![ContentDependency {
        version_id: None,
        project_id: Some("dependency".into()),
        file_name: None,
        dependency_type: "required".into(),
    }];
    provider.versions.push(newer);
    let attached = installed
        .iter()
        .find(|r| r.directory == "resourcepacks")
        .unwrap();
    let selections = vec![ContentSelection {
        directory: attached.directory.clone(),
        filename: attached.file.filename.clone(),
        sha512: attached.file.hashes.sha512.clone(),
    }];
    let plan = update::plan(&provider, &instance, &installed, &[], &selections).unwrap();
    assert_eq!(plan.previous.len(), 3);
    assert_eq!(plan.plan.files.len(), 5);
    for world in ["one", "two"] {
        assert_eq!(
            plan.plan
                .files
                .iter()
                .filter(|r| r.directory == format!("saves/{world}/datapacks"))
                .count(),
            2
        );
    }
    let pinned = ContentUpdatePolicy {
        project_id: "parent".into(),
        pinned: true,
        ..Default::default()
    };
    assert!(update::plan(&provider, &instance, &installed, &[pinned], &selections).is_err());
}
#[test]
fn datapack_format_ranges_validate_types_order_and_minor_versions() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("data.zip");
    for (format, valid) in [
        (r#""min_format":48,"max_format":61"#, true),
        (r#""min_format":[61,0],"max_format":[61,1]"#, true),
        (r#""min_format":61,"max_format":48"#, false),
        (r#""min_format":"48","max_format":{}"#, false),
        (r#""min_format":[61,-1],"max_format":[61,1]"#, false),
    ] {
        let body = format!("{{\"pack\":{{{format}}}}}");
        local_jar(
            &source,
            &[
                ("pack.mcmeta", &body),
                ("data/test/function/load.mcfunction", "say test"),
            ],
        );
        assert_eq!(worlds::validate_datapack(&source).is_ok(), valid);
    }
}
#[test]
fn datapack_dependencies_reuse_verified_installed_versions_and_conflicts_are_world_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let (mut manager, instance, directory) = adoption_fixture(temp.path());
    create_world(&directory, "one");
    create_world(&directory, "two");
    let paths = Paths::new(temp.path()).unwrap();
    let mut provider = fixture();
    for version in &mut provider.versions {
        version.loaders = vec!["datapack".into()];
        version.files[0].filename = format!("{}.zip", version.project_id);
    }
    provider.versions[0].dependencies.push(ContentDependency {
        project_id: Some("blocked".into()),
        version_id: None,
        file_name: None,
        dependency_type: "incompatible".into(),
    });
    let mut latest = provider.versions[1].clone();
    latest.id = "newDependency".into();
    latest.date_published = "2026-10-01".into();
    provider.versions.push(latest);
    let mut installed = worlds::selected(
        &provider.projects["dependency"],
        &provider.versions[1],
        "one",
        true,
    )
    .unwrap();
    let mut blocker = installed[0].clone();
    blocker.project_id = "blocked".into();
    blocker.version.project_id = "blocked".into();
    blocker.file.filename = "blocked.zip".into();
    blocker.directory = "saves/two/datapacks".into();
    installed.push(blocker.clone());
    for record in &installed {
        let target = install::target(&paths, &directory, record).unwrap();
        paths.mkdir(target.parent().unwrap()).unwrap();
        std::fs::write(target, b"verified test content").unwrap();
    }
    install::write_records(&paths, &directory, &installed).unwrap();
    manager.provider = Arc::new(provider);
    let request = || WorldProjectRequest {
        instance_id: instance.id.clone(),
        project_id: "parent".into(),
        version_id: "parentVersion".into(),
        world: "one".into(),
    };
    let plan = manager.world_project_plan(request()).unwrap();
    assert_eq!(
        plan.files
            .iter()
            .find(|r| r.project_id == "dependency")
            .unwrap()
            .version
            .id,
        "depVersion"
    );
    blocker.directory = "saves/one/datapacks".into();
    installed.push(blocker);
    install::write_records(&paths, &directory, &installed).unwrap();
    assert!(matches!(
        manager.world_project_plan(request()),
        Err(CoreError::DependencyConflict)
    ));
}
#[test]
fn deep_datapack_update_dependencies_reach_worlds_discovered_after_first_visit() {
    let temp = tempfile::tempdir().unwrap();
    let (_, instance) = library(temp.path());
    let mut provider = fixture();
    provider.versions[0].dependencies.clear();
    for version in &mut provider.versions {
        version.loaders = vec!["datapack".into()];
        version.files[0].filename = format!("{}.zip", version.project_id);
    }
    let mut installed = worlds::selected(
        &provider.projects["parent"],
        &provider.versions[0],
        "one",
        false,
    )
    .unwrap();
    installed.extend(
        worlds::selected(
            &provider.projects["dependency"],
            &provider.versions[1],
            "two",
            false,
        )
        .unwrap(),
    );
    for (id, child) in [("middleA", "middleB"), ("middleB", "dependency")] {
        let mut project = provider.projects["parent"].clone();
        project.id = id.into();
        provider.projects.insert(id.into(), project);
        let mut v = version(id, id, Some(child));
        v.loaders = vec!["datapack".into()];
        v.files[0].filename = format!("{id}.zip");
        provider.versions.push(v);
    }
    let mut root = provider.versions[0].clone();
    root.id = "parentNew".into();
    root.dependencies = vec![ContentDependency {
        project_id: Some("middleA".into()),
        version_id: None,
        file_name: None,
        dependency_type: "required".into(),
    }];
    provider.versions.push(root);
    let requests = vec![
        ContentRequest {
            instance_id: instance.id.clone(),
            project_id: "parent".into(),
            version_id: "parentNew".into(),
        },
        ContentRequest {
            instance_id: instance.id.clone(),
            project_id: "dependency".into(),
            version_id: "depVersion".into(),
        },
    ];
    let plan = resolve::resolve_many(&provider, &requests, &instance, &installed, true).unwrap();
    for world in ["one", "two"] {
        assert!(
            plan.files.iter().any(|r| r.project_id == "dependency"
                && r.directory == format!("saves/{world}/datapacks"))
        );
    }
    assert_eq!(plan.files.len(), 5);
}
fn matched_fixture(
    root: &std::path::Path,
) -> (
    ContentManager,
    Instance,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let (library, instance) = library(root);
    let paths = Paths::new(root).unwrap();
    let directory = paths.location("instances", &instance.id).unwrap();
    let source = root.join("renamed-root.jar");
    let dep = root.join("dependency.jar");
    diagnostic_jar(
        &source,
        "parent",
        "1.0",
        serde_json::json!({"depends":{"minecraft":"1.21.1","dependency":">=2"}}),
    );
    diagnostic_jar(&dep, "dependency", "2.0", serde_json::json!({}));
    let mut provider = fixture();
    for (version, path) in provider.versions.iter_mut().zip([&source, &dep]) {
        let file = worlds::artifact(path, &version.files[0].filename).unwrap();
        version.files[0].size = file.size;
        version.files[0].hashes = file.hashes;
    }
    let mut manager = ContentManager::new(library).unwrap();
    manager.provider = Arc::new(provider);
    (manager, instance, directory, source, dep)
}
#[test]
fn matched_local_import_can_review_dependencies_cancel_or_commit_without_rewriting_original() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory, source, dep) = matched_fixture(temp.path());
    let paths = Paths::new(temp.path()).unwrap();
    let original = std::fs::read(&source).unwrap();
    let preview = manager
        .local_plan(&instance.id, vec![source.to_string_lossy().into()])
        .unwrap();
    std::fs::copy(
        &dep,
        install::stage(&paths, &directory, &preview.plan.token, 1).unwrap(),
    )
    .unwrap();
    let preview = manager.local_dependencies(&preview.plan.token).unwrap();
    assert_eq!(preview.plan.files.len(), 2);
    assert!(preview.plan.files.iter().all(|r| r.provider == "modrinth"));
    assert_eq!(preview.plan.files[0].file.filename, "renamed-root.jar");
    assert_eq!(preview.diagnostics.errors, 0);
    assert!(!directory.join("mods/renamed-root.jar").exists());
    manager
        .local_finish(&preview.plan.token, false, true)
        .unwrap();
    assert!(manager.installed(&instance.id).unwrap().is_empty());
    let preview = manager
        .local_plan(&instance.id, vec![source.to_string_lossy().into()])
        .unwrap();
    std::fs::copy(
        &dep,
        install::stage(&paths, &directory, &preview.plan.token, 1).unwrap(),
    )
    .unwrap();
    let preview = manager.local_dependencies(&preview.plan.token).unwrap();
    manager
        .local_finish(&preview.plan.token, true, false)
        .unwrap();
    assert_eq!(
        std::fs::read(directory.join("mods/renamed-root.jar")).unwrap(),
        original
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert_eq!(manager.installed(&instance.id).unwrap().len(), 2);
    assert_eq!(manager.diagnostics(&instance.id).unwrap().errors, 0);
}
#[test]
fn managed_local_dependency_plan_retains_local_provenance_and_refuses_stale_context() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory, source, _) = matched_fixture(temp.path());
    let preview = manager
        .local_plan(&instance.id, vec![source.to_string_lossy().into()])
        .unwrap();
    manager
        .local_finish(&preview.plan.token, true, false)
        .unwrap();
    let rows = manager.installed(&instance.id).unwrap();
    let plan = manager
        .dependency_plan(&instance.id, selection(&rows))
        .unwrap();
    assert_eq!(plan.parents[0].provider, "modrinth");
    assert_eq!(plan.plan.files.len(), 1);
    assert_eq!(
        manager.installed(&instance.id).unwrap()[0].record.provider,
        "local"
    );
    diagnostic_jar(
        &directory.join("mods/new.jar"),
        "new",
        "1",
        serde_json::json!({}),
    );
    assert!(matches!(
        manager.start(&plan.plan.token),
        Err(CoreError::RecordConflict)
    ));
    assert_eq!(manager.installed(&instance.id).unwrap().len(), 1);
}
#[test]
fn dependency_identity_never_uses_names_or_hash_only_without_sha1_size_or_compatibility() {
    let temp = tempfile::tempdir().unwrap();
    let (mut manager, instance, _, source, _) = matched_fixture(temp.path());
    let preview = manager
        .local_plan(&instance.id, vec![source.to_string_lossy().into()])
        .unwrap();
    let root = preview.plan.files[0].clone();
    let mut provider = fixture();
    assert!(
        dependencies::matched(&provider, &instance, &root)
            .unwrap()
            .is_none()
    );
    let mut version = manager.provider.version("parentVersion").unwrap();
    version.files[0].hashes.sha1 = "f".repeat(40);
    provider.versions[0] = version;
    assert!(matches!(
        dependencies::matched(&provider, &instance, &root),
        Err(CoreError::Integrity)
    ));
    manager.provider = Arc::new(UnavailableProvider);
    assert!(matches!(
        manager.local_dependencies(&preview.plan.token),
        Err(CoreError::Network)
    ));
    manager
        .local_finish(&preview.plan.token, true, false)
        .unwrap();
    assert_eq!(
        manager.installed(&instance.id).unwrap()[0].record.provider,
        "local"
    );
}

fn diagnostic_jar(path: &std::path::Path, id: &str, version: &str, extra: serde_json::Value) {
    let mut json = serde_json::json!({"schemaVersion":1,"id":id,"name":id,"version":version,"environment":"client","depends":{"minecraft":"1.21.1"}});
    for (key, value) in extra.as_object().unwrap() {
        json[key] = value.clone();
    }
    let body = serde_json::to_string(&json).unwrap();
    local_jar(path, &[("fabric.mod.json", &body)]);
}
fn diagnostic_check<'a>(report: &'a ModDiagnostics, file: &str, id: &str) -> &'a DependencyCheck {
    report
        .mods
        .iter()
        .find(|m| m.filename == file)
        .unwrap()
        .checks
        .iter()
        .find(|c| c.dependency.id == id)
        .unwrap()
}

#[test]
fn diagnostics_are_read_only_and_distinguish_missing_disabled_versions_conflicts_and_optional() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    diagnostic_jar(
        &directory.join("mods/owner.jar"),
        "owner",
        "1.0",
        serde_json::json!({"depends":{"absent":"*","disabled_lib":">=1","wrong_lib":">=2"},"breaks":{"bad_lib":"*"},"conflicts":{"manual":"*"},"suggests":{"optional_absent":">=2"}}),
    );
    for (folder, id) in [
        ("mods_disabled", "disabled_lib"),
        ("mods", "wrong_lib"),
        ("mods", "bad_lib"),
    ] {
        diagnostic_jar(
            &directory.join(folder).join(format!("{id}.jar")),
            id,
            "1.0",
            serde_json::json!({}),
        );
    }
    let original = std::fs::read(directory.join("mods/owner.jar")).unwrap();
    let report = manager.diagnostics(&instance.id).unwrap();
    assert!(report.complete);
    for (id, expected) in [
        ("absent", "missing"),
        ("disabled_lib", "disabled"),
        ("wrong_lib", "version_mismatch"),
        ("bad_lib", "conflict"),
        ("manual", "conflict"),
        ("optional_absent", "optional"),
    ] {
        assert_eq!(diagnostic_check(&report, "owner.jar", id).status, expected);
    }
    assert_eq!(report.errors, 4);
    assert_eq!(
        diagnostic_check(&report, "owner.jar", "optional_absent").severity,
        "info"
    );
    assert_eq!(
        std::fs::read(directory.join("mods/owner.jar")).unwrap(),
        original
    );
    assert!(manager.installed(&instance.id).unwrap().is_empty());
    assert!(manager.history(&instance.id).unwrap().is_empty());
}

#[test]
fn diagnostics_do_not_fake_missing_nested_or_unreadable_providers_and_alias_versions() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    diagnostic_jar(
        &directory.join("mods/owner.jar"),
        "owner",
        "1",
        serde_json::json!({"depends":{"absent":"*","alias_id":">=1","java":">=21"}}),
    );
    diagnostic_jar(
        &directory.join("mods/provider.jar"),
        "provider",
        "3.0",
        serde_json::json!({"provides":["alias_id"],"jars":[{"file":"nested.jar"}]}),
    );
    let report = manager.diagnostics(&instance.id).unwrap();
    assert!(!report.complete);
    for id in ["absent", "alias_id", "java"] {
        assert_eq!(diagnostic_check(&report, "owner.jar", id).status, "unknown");
    }
    diagnostic_jar(
        &directory.join("mods/owner.jar"),
        "owner",
        "1",
        serde_json::json!({"depends":{"alias_id":"*"}}),
    );
    assert_eq!(
        diagnostic_check(
            &manager.diagnostics(&instance.id).unwrap(),
            "owner.jar",
            "alias_id"
        )
        .status,
        "satisfied"
    );
    std::fs::write(directory.join("mods/provider.jar"), b"unreadable provider").unwrap();
    let report = manager.diagnostics(&instance.id).unwrap();
    assert!(!report.complete);
    assert_eq!(
        diagnostic_check(&report, "owner.jar", "alias_id").status,
        "unknown"
    );
}

#[test]
fn diagnostics_use_actual_individual_versions_and_client_dependency_types_in_neoforge() {
    let temp = tempfile::tempdir().unwrap();
    let (_, mut instance) = library(temp.path());
    instance.loader = Loader::NeoForge;
    instance.loader_version = Some("21.1.252".into());
    let paths = Paths::new(temp.path()).unwrap();
    let directory = paths.location("instances", &instance.id).unwrap();
    local_jar(
        &directory.join("mods/multi.jar"),
        &[(
            "META-INF/neoforge.mods.toml",
            "modLoader='javafml'\n[[mods]]\nmodId='one'\nversion='1.0'\n[[mods]]\nmodId='two'\nversion='2.0'\n[[dependencies.one]]\nmodId='two'\ntype='required'\nversionRange='[2,3)'\nside='CLIENT'\n[[dependencies.one]]\nmodId='server_lib'\ntype='required'\nside='SERVER'\n[[dependencies.two]]\nmodId='one'\ntype='incompatible'\nversionRange='[1,2)'\n[[dependencies.two]]\nmodId='absent_optional'\ntype='optional'\n[[dependencies.two]]\nmodId='one'\ntype='optional'\nversionRange='[3,4)'",
        )],
    );
    let report = diagnostics::scan(&paths, &directory, &instance, &[]).unwrap();
    let row = &report.mods[0];
    assert!(row.checks.iter().all(|c| c.dependency.id != "server_lib"));
    let two = row
        .checks
        .iter()
        .find(|c| c.dependency.id == "two")
        .unwrap();
    assert_eq!(two.status, "satisfied");
    assert_eq!(two.targets[0].version.as_deref(), Some("2.0"));
    assert!(
        row.checks
            .iter()
            .any(|c| c.dependency.owner_id == "two" && c.status == "conflict")
    );
    assert!(
        row.checks
            .iter()
            .any(|c| c.dependency.relation == "optional"
                && c.status == "version_mismatch"
                && c.severity == "error")
    );
    assert_eq!(
        row.checks
            .iter()
            .find(|c| c.dependency.id == "absent_optional")
            .unwrap()
            .severity,
        "info"
    );
}

#[test]
fn diagnostics_numeric_predicates_follow_fabric_arrays_builds_caret_and_maven_bounds_conservatively()
 {
    let check = |v, ranges: &[&str], dialect| {
        diagnostics::matches(
            v,
            &ranges.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            dialect,
        )
    };
    assert_eq!(
        check(Some("1.4.2+mc1.21"), &[">=1.4 <2"], "fabric"),
        Some(true)
    );
    assert_eq!(check(Some("0.8.0"), &["^0.1.0"], "fabric"), Some(true));
    assert_eq!(check(Some("2.0.0"), &["^1.3.0"], "fabric"), Some(false));
    assert_eq!(check(Some("1.3.0"), &["~1.2.0"], "fabric"), Some(false));
    assert_eq!(
        check(Some("1.3.0"), &["1.2.X", "1.3.x"], "fabric"),
        Some(true)
    );
    assert_eq!(check(Some("1.0-beta"), &[">=1"], "fabric"), None);
    assert_eq!(check(Some("26w14a"), &["26w14a"], "fabric"), Some(true));
    assert_eq!(check(Some("2.0"), &["[1,2)"], "maven"), Some(false));
    assert_eq!(check(Some("2.0"), &["2.0"], "maven"), None);
    assert_eq!(check(None, &["*"], "fabric"), Some(true));
    assert_eq!(check(None, &[">=1"], "fabric"), None);
    assert_eq!(
        check(Some("2"), &[">=3", "custom || syntax"], "fabric"),
        None
    );
}

#[test]
fn staged_batch_dependencies_are_checked_together_and_changed_manual_mods_invalidate_acceptance() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    let source = temp.path().join("owner.jar");
    let dependency = temp.path().join("needed.jar");
    diagnostic_jar(
        &source,
        "new_owner",
        "1",
        serde_json::json!({"depends":{"needed":">=2"}}),
    );
    diagnostic_jar(&dependency, "needed", "2.0", serde_json::json!({}));
    let preview = manager
        .local_plan(
            &instance.id,
            vec![
                source.to_string_lossy().into(),
                dependency.to_string_lossy().into(),
            ],
        )
        .unwrap();
    assert_eq!(
        diagnostic_check(&preview.diagnostics, "owner.jar", "needed").status,
        "satisfied"
    );
    assert!(preview.warnings.iter().all(|w| w != "dependency_issues"));
    diagnostic_jar(
        &directory.join("mods/unrelated.jar"),
        "unrelated",
        "1",
        serde_json::json!({}),
    );
    assert!(matches!(
        manager.local_finish(&preview.plan.token, true, false),
        Err(CoreError::RecordConflict)
    ));
    assert!(!directory.join("mods/owner.jar").exists());
    manager
        .local_finish(&preview.plan.token, false, true)
        .unwrap();
    let preview = manager
        .local_plan(
            &instance.id,
            vec![
                source.to_string_lossy().into(),
                dependency.to_string_lossy().into(),
            ],
        )
        .unwrap();
    manager
        .local_finish(&preview.plan.token, true, false)
        .unwrap();
    assert_eq!(
        diagnostic_check(
            &manager.diagnostics(&instance.id).unwrap(),
            "owner.jar",
            "needed"
        )
        .status,
        "satisfied"
    );
    let preview = manager.adoption_plan(adoption_request(&instance)).unwrap();
    std::fs::remove_file(directory.join("mods/needed.jar")).unwrap();
    assert!(matches!(
        manager.adoption_finish(&preview.plan.token, true, false),
        Err(CoreError::RecordConflict)
    ));
}

#[test]
fn disabled_owners_do_not_count_as_active_errors_and_duplicate_primary_ids_are_visible() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    diagnostic_jar(
        &directory.join("mods_disabled/disabled.jar"),
        "disabled",
        "1",
        serde_json::json!({"depends":{"missing":"*"}}),
    );
    assert_eq!(manager.diagnostics(&instance.id).unwrap().errors, 0);
    diagnostic_jar(
        &directory.join("mods/copy.jar"),
        "manual",
        "9.0",
        serde_json::json!({}),
    );
    let report = manager.diagnostics(&instance.id).unwrap();
    assert!(
        report
            .mods
            .iter()
            .filter(|m| m.enabled)
            .all(|m| m.warnings.iter().any(|w| w == "duplicate_mod_id"))
    );
}

#[test]
fn unknown_forge_dependency_side_does_not_claim_known_client_incompatibility() {
    let temp = tempfile::tempdir().unwrap();
    let (_, mut instance) = library(temp.path());
    instance.loader = Loader::Forge;
    let file = temp.path().join("unknown-side.jar");
    local_jar(
        &file,
        &[(
            "META-INF/mods.toml",
            "modLoader='javafml'\n[[mods]]\nmodId='unknown_side'\nversion='1.0'\n[[dependencies.unknown_side]]\nmodId='minecraft'\nmandatory=true\nversionRange='[1.20,1.21)'\nside='UNKNOWN'",
        )],
    );
    let (_, _, metadata) = local::inspect(&file, &instance).unwrap();
    assert_eq!(metadata.dependencies[0].relation, "unknown");
    assert!(metadata.warnings.iter().any(|w| w == "unknown_dependency"));
}

#[test]
fn local_import_confirmation_binds_loader_context_even_without_receipt_changes() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    let source = temp.path().join("context.jar");
    diagnostic_jar(&source, "context_probe", "1.0", serde_json::json!({}));
    let preview = manager
        .local_plan(&instance.id, vec![source.to_string_lossy().into()])
        .unwrap();
    manager
        .library
        .configure_launch(crate::instances::model::ConfigureLaunch {
            id: instance.id.clone(),
            expected_revision: instance.revision,
            loader_version: Some("0.19.5".into()),
        })
        .unwrap();
    assert!(matches!(
        manager.local_finish(&preview.plan.token, true, false),
        Err(CoreError::RecordConflict)
    ));
    assert!(!directory.join("mods/context.jar").exists());
    manager
        .local_finish(&preview.plan.token, false, true)
        .unwrap();
}

fn adoption_fixture(root: &std::path::Path) -> (ContentManager, Instance, std::path::PathBuf) {
    let (library, instance) = library(root);
    let directory = Paths::new(root)
        .unwrap()
        .location("instances", &instance.id)
        .unwrap();
    local_jar(
        &directory.join("mods/manual.jar"),
        &[(
            "fabric.mod.json",
            r#"{"schemaVersion":1,"id":"manual","name":"Manual","version":"1.0","environment":"client","depends":{"minecraft":"1.21.1"}}"#,
        )],
    );
    let mut manager = ContentManager::new(library).unwrap();
    manager.provider = Arc::new(fixture());
    (manager, instance, directory)
}
fn adoption_request(instance: &Instance) -> ContentAdoptionRequest {
    ContentAdoptionRequest {
        instance_id: instance.id.clone(),
        directory: "mods".into(),
        filename: "manual.jar".into(),
        recognize: false,
    }
}

struct UnavailableProvider;
impl ContentProvider for UnavailableProvider {
    fn version_from_hash(&self, _: &str) -> Result<Option<ContentVersion>, CoreError> {
        Err(CoreError::Network)
    }
    fn search(&self, _: &CatalogQuery) -> Result<CatalogPage, CoreError> {
        unreachable!()
    }
    fn tags(&self) -> Result<ContentTags, CoreError> {
        unreachable!()
    }
    fn project(&self, _: &str) -> Result<ContentProject, CoreError> {
        unreachable!()
    }
    fn version(&self, _: &str) -> Result<ContentVersion, CoreError> {
        unreachable!()
    }
    fn versions(&self, _: &str, _: &str) -> Result<Vec<ContentVersion>, CoreError> {
        unreachable!()
    }
}
#[test]
fn adoption_is_available_without_network_and_failed_identification_requires_local_confirmation() {
    let temp = tempfile::tempdir().unwrap();
    let (mut manager, instance, _) = adoption_fixture(temp.path());
    manager.provider = Arc::new(UnavailableProvider);
    let offline = manager.adoption_plan(adoption_request(&instance)).unwrap();
    assert!(offline.warnings.is_empty());
    manager
        .adoption_finish(&offline.plan.token, false, true)
        .unwrap();
    let mut request = adoption_request(&instance);
    request.recognize = true;
    let preview = manager.adoption_plan(request).unwrap();
    assert_eq!(preview.plan.files[0].provider, "local");
    assert_eq!(preview.warnings, ["match_unavailable"]);
    assert!(matches!(
        manager.adoption_finish(&preview.plan.token, false, false),
        Err(CoreError::ContentIncompatible)
    ));
    manager
        .adoption_finish(&preview.plan.token, true, false)
        .unwrap();
    assert_eq!(
        manager.installed(&instance.id).unwrap()[0].record.provider,
        "local"
    );
}

#[test]
fn adoption_cancel_keeps_bytes_and_receipt_then_explicit_commit_survives_restart_and_management() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    let bytes = std::fs::read(directory.join("mods/manual.jar")).unwrap();
    let preview = manager.adoption_plan(adoption_request(&instance)).unwrap();
    assert!(manager.installed(&instance.id).unwrap().is_empty());
    assert!(manager.history(&instance.id).unwrap().is_empty());
    assert_eq!(manager.untracked(&instance.id).unwrap().len(), 1);
    manager
        .adoption_finish(&preview.plan.token, false, true)
        .unwrap();
    assert!(matches!(
        manager.adoption_finish(&preview.plan.token, false, false),
        Err(CoreError::NotFound)
    ));
    assert_eq!(
        std::fs::read(directory.join("mods/manual.jar")).unwrap(),
        bytes
    );
    let preview = manager.adoption_plan(adoption_request(&instance)).unwrap();
    manager
        .adoption_finish(&preview.plan.token, false, false)
        .unwrap();
    assert_eq!(
        std::fs::read(directory.join("mods/manual.jar")).unwrap(),
        bytes
    );
    let restarted = ContentManager::new(manager.library.clone()).unwrap();
    let rows = restarted.installed(&instance.id).unwrap();
    assert_eq!(rows[0].record.provider, "local");
    assert_eq!(rows[0].status, "installed");
    assert!(restarted.untracked(&instance.id).unwrap().is_empty());
    assert_eq!(restarted.history(&instance.id).unwrap()[0].action, "adopt");
    restarted
        .change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Disable,
            files: selection(&rows),
        })
        .unwrap();
    assert_eq!(
        std::fs::read(directory.join("mods_disabled/manual.jar")).unwrap(),
        bytes
    );
    let rows = restarted.installed(&instance.id).unwrap();
    restarted
        .change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Enable,
            files: selection(&rows),
        })
        .unwrap();
    let rows = restarted.installed(&instance.id).unwrap();
    restarted
        .change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Delete,
            files: selection(&rows),
        })
        .unwrap();
    assert!(!directory.join("mods/manual.jar").exists());
    assert_eq!(restarted.history(&instance.id).unwrap().len(), 4);
}

#[test]
fn adoption_refuses_changed_bytes_context_receipt_unknown_warnings_and_active_lease() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    let preview = manager.adoption_plan(adoption_request(&instance)).unwrap();
    let original = std::fs::read(directory.join("mods/manual.jar")).unwrap();
    std::fs::write(directory.join("mods/manual.jar"), b"changed").unwrap();
    assert!(
        manager
            .adoption_finish(&preview.plan.token, false, false)
            .is_err()
    );
    assert!(manager.installed(&instance.id).unwrap().is_empty());
    std::fs::write(directory.join("mods/manual.jar"), original).unwrap();
    let (_, _, lease) = manager.library.lease_game(&instance.id).unwrap();
    assert!(matches!(
        manager.adoption_finish(&preview.plan.token, false, false),
        Err(CoreError::InstanceBusy)
    ));
    drop(lease);
    manager
        .state
        .lock()
        .unwrap()
        .adoption_plans
        .get_mut(&preview.plan.token)
        .unwrap()
        .3 = b"stale context".to_vec();
    assert!(matches!(
        manager.adoption_finish(&preview.plan.token, false, false),
        Err(CoreError::RecordConflict)
    ));
    let preview = manager.adoption_plan(adoption_request(&instance)).unwrap();
    manager
        .state
        .lock()
        .unwrap()
        .adoption_plans
        .get_mut(&preview.plan.token)
        .unwrap()
        .2 = b"stale receipt".to_vec();
    assert!(matches!(
        manager.adoption_finish(&preview.plan.token, false, false),
        Err(CoreError::RecordConflict)
    ));
    local_jar(
        &directory.join("mods/manual.jar"),
        &[("readme.txt", "unknown archive")],
    );
    let preview = manager.adoption_plan(adoption_request(&instance)).unwrap();
    assert!(!preview.warnings.is_empty());
    assert!(matches!(
        manager.adoption_finish(&preview.plan.token, false, false),
        Err(CoreError::ContentIncompatible)
    ));
    manager
        .adoption_finish(&preview.plan.token, true, false)
        .unwrap();
}

#[test]
fn adoption_rejects_paths_corruption_wrong_version_wrong_loader_and_duplicate_ids() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    for (folder, filename) in [
        ("../mods", "manual.jar"),
        ("mods", "../manual.jar"),
        ("config", "manual.jar"),
        ("mods", "manual.zip"),
    ] {
        let mut request = adoption_request(&instance);
        request.directory = folder.into();
        request.filename = filename.into();
        assert!(manager.adoption_plan(request).is_err());
    }
    local_jar(
        &directory.join("mods/other.jar"),
        &[(
            "fabric.mod.json",
            r#"{"schemaVersion":1,"id":"manual","version":"1.0","depends":{"minecraft":"1.21.1"}}"#,
        )],
    );
    assert!(matches!(
        manager.adoption_plan(adoption_request(&instance)),
        Err(CoreError::ContentConflict)
    ));
    std::fs::remove_file(directory.join("mods/other.jar")).unwrap();
    for metadata in [
        r#"{"schemaVersion":1,"id":"manual","depends":{"minecraft":"1.20.1"}}"#,
        r#"{"schemaVersion":1,"id":"manual","environment":"server"}"#,
    ] {
        local_jar(
            &directory.join("mods/manual.jar"),
            &[("fabric.mod.json", metadata)],
        );
        assert!(matches!(
            manager.adoption_plan(adoption_request(&instance)),
            Err(CoreError::ContentIncompatible)
        ));
    }
    local_jar(
        &directory.join("mods/manual.jar"),
        &[(
            "META-INF/mods.toml",
            "modLoader = 'javafml'\nloaderVersion = '[1,)'\n[[mods]]\nmodId = 'forge_only'\nversion = '1'",
        )],
    );
    assert!(matches!(
        manager.adoption_plan(adoption_request(&instance)),
        Err(CoreError::ContentIncompatible)
    ));
    std::fs::write(directory.join("mods/manual.jar"), b"corrupt").unwrap();
    assert!(matches!(
        manager.adoption_plan(adoption_request(&instance)),
        Err(CoreError::Integrity)
    ));
    assert!(manager.installed(&instance.id).unwrap().is_empty());
}

#[test]
fn adoption_exact_hash_match_keeps_renamed_filename_and_official_provenance() {
    let temp = tempfile::tempdir().unwrap();
    let (mut manager, instance, directory) = adoption_fixture(temp.path());
    local_jar(
        &directory.join("mods/manual.jar"),
        &[(
            "fabric.mod.json",
            r#"{"schemaVersion":1,"id":"manual","version":"1.0","environment":"client","depends":{"minecraft":"1.21.1","other_api":"*"}}"#,
        )],
    );
    let bytes = std::fs::read(directory.join("mods/manual.jar")).unwrap();
    let mut provider = fixture();
    let v = &mut provider.versions[0];
    v.dependencies.clear();
    v.files[0].size = bytes.len() as u64;
    v.files[0].hashes.sha1 = hex(&sha1::Sha1::digest(&bytes));
    v.files[0].hashes.sha512 = hex(&sha2::Sha512::digest(&bytes));
    manager.provider = Arc::new(provider);
    let mut request = adoption_request(&instance);
    request.recognize = true;
    let preview = manager.adoption_plan(request).unwrap();
    let record = &preview.plan.files[0];
    assert_eq!(preview.metadata.mod_ids, ["manual"]);
    assert_eq!(preview.metadata.required, ["other_api"]);
    assert_eq!(record.provider, "modrinth");
    assert!(record.local.is_none());
    assert_eq!(record.file.filename, "manual.jar");
    assert_eq!(record.version.files[0].filename, "parent.jar");
    assert_eq!(record.project_id, "parent");
    assert!(record.file.url.starts_with("https://cdn.modrinth.com/"));
    manager
        .adoption_finish(&preview.plan.token, true, false)
        .unwrap();
    assert_eq!(
        std::fs::read(directory.join("mods/manual.jar")).unwrap(),
        bytes
    );
    let restarted = ContentManager::new(manager.library.clone()).unwrap();
    assert_eq!(
        restarted.installed(&instance.id).unwrap()[0]
            .record
            .provider,
        "modrinth"
    );
}

#[test]
fn adoption_does_not_trust_hash_result_without_sha1_size_compatibility_or_safe_url() {
    let temp = tempfile::tempdir().unwrap();
    let (mut manager, instance, directory) = adoption_fixture(temp.path());
    let bytes = std::fs::read(directory.join("mods/manual.jar")).unwrap();
    for case in ["sha1", "size", "url", "game", "loader", "source"] {
        let mut provider = fixture();
        let v = &mut provider.versions[0];
        v.files[0].size = bytes.len() as u64;
        v.files[0].hashes.sha1 = hex(&sha1::Sha1::digest(&bytes));
        v.files[0].hashes.sha512 = hex(&sha2::Sha512::digest(&bytes));
        match case {
            "sha1" => v.files[0].hashes.sha1 = "f".repeat(40),
            "size" => v.files[0].size += 1,
            "url" => v.files[0].url = "https://example.com/file.jar".into(),
            "game" => v.game_versions = vec!["1.20.1".into()],
            "loader" => v.loaders = vec!["forge".into()],
            _ => v.files[0].file_type = Some("sources-jar".into()),
        }
        manager.provider = Arc::new(provider);
        let mut request = adoption_request(&instance);
        request.recognize = true;
        assert!(manager.adoption_plan(request).is_err(), "{case}");
    }
    assert!(manager.installed(&instance.id).unwrap().is_empty());
}

#[test]
fn adoption_missing_hash_stays_local_and_disabled_zip_pack_management_is_supported() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    std::fs::rename(
        directory.join("mods/manual.jar"),
        directory.join("mods_disabled/manual.jar"),
    )
    .unwrap();
    let mut request = adoption_request(&instance);
    request.directory = "mods_disabled".into();
    request.recognize = true;
    let plan = manager.adoption_plan(request).unwrap();
    assert_eq!(plan.plan.files[0].provider, "local");
    assert!(plan.warnings.iter().any(|w| w == "match_not_found"));
    manager
        .adoption_finish(&plan.plan.token, true, false)
        .unwrap();
    assert_eq!(
        manager.installed(&instance.id).unwrap()[0].status,
        "disabled"
    );
    for folder in ["resourcepacks", "shaderpacks"] {
        local_jar(
            &directory.join(folder).join("manual.zip"),
            &[(
                "pack.mcmeta",
                "{\"pack\":{\"pack_format\":34,\"description\":\"Manual\"}}",
            )],
        );
        let mut request = adoption_request(&instance);
        request.directory = folder.into();
        request.filename = "manual.zip".into();
        let preview = manager.adoption_plan(request).unwrap();
        manager
            .adoption_finish(&preview.plan.token, true, false)
            .unwrap();
    }
    let rows = manager.installed(&instance.id).unwrap();
    assert_eq!(rows.len(), 3);
    manager
        .change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Delete,
            files: selection(&rows),
        })
        .unwrap();
    assert!(manager.installed(&instance.id).unwrap().is_empty());
}

#[test]
fn adoption_recovery_rolls_forward_receipt_only_and_history_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    let preview = manager.adoption_plan(adoption_request(&instance)).unwrap();
    let record = preview.plan.files[0].clone();
    let journal = serde_json::json!({"schema":1,"before":[],"added":record,"event":{"id":uuid::Uuid::new_v4().to_string(),"timestamp":1,"action":"adopt","titles":["Manual"]}});
    let file = directory.join(".sporium/content-adoption.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let bytes = std::fs::read(directory.join("mods/manual.jar")).unwrap();
    std::fs::write(&file, serde_json::to_vec(&journal).unwrap()).unwrap();
    assert_eq!(manager.installed(&instance.id).unwrap().len(), 1);
    assert!(!file.exists());
    std::fs::write(&file, serde_json::to_vec(&journal).unwrap()).unwrap();
    assert_eq!(manager.installed(&instance.id).unwrap().len(), 1);
    assert_eq!(manager.history(&instance.id).unwrap().len(), 1);
    assert_eq!(
        std::fs::read(directory.join("mods/manual.jar")).unwrap(),
        bytes
    );
}

#[test]
fn archive_icons_are_bounded_png_and_never_extract_files_or_allow_metadata_paths() {
    use base64::Engine;
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = adoption_fixture(temp.path());
    let png = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRzUAAAAASUVORK5CYII=").unwrap();
    let file = directory.join("mods/manual.jar");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&file).unwrap());
    for (name, bytes) in [("fabric.mod.json", br#"{"schemaVersion":1,"id":"manual","version":"1.0","icon":{"16":"assets/icon.png"},"environment":"client","depends":{"minecraft":"1.21.1"}}"#.as_slice()), ("assets/icon.png", png.as_slice())] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap(); zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    let icon = manager
        .local_icon(&instance.id, "mods", "manual.jar", None)
        .unwrap()
        .unwrap();
    assert!(icon.starts_with("data:image/png;base64,"));
    assert!(!directory.join("assets/icon.png").exists());
    let preview = manager.adoption_plan(adoption_request(&instance)).unwrap();
    manager
        .adoption_finish(&preview.plan.token, false, false)
        .unwrap();
    let hash = &preview.plan.files[0].file.hashes.sha512;
    assert_eq!(
        manager
            .local_icon(&instance.id, "mods", "manual.jar", Some(hash))
            .unwrap(),
        Some(icon)
    );
    local_jar(
        &file,
        &[("fabric.mod.json", r#"{"icon":"../outside.png"}"#)],
    );
    assert!(
        manager
            .local_icon(&instance.id, "mods", "manual.jar", None)
            .is_err()
    );
    assert!(matches!(
        manager.local_icon(&instance.id, "mods", "manual.jar", Some(hash)),
        Err(CoreError::SourceChanged)
    ));
}
#[test]
fn local_import_stages_exact_bytes_preserves_source_and_migrates_mixed_receipts() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = installed_fixture(temp.path());
    let source = temp.path().join("my-local.jar");
    local_jar(
        &source,
        &[(
            "fabric.mod.json",
            r#"{"schemaVersion":1,"id":"local_test","name":"Local test","version":"2.0","environment":"client","depends":{"minecraft":">=1.21 <1.22"}}"#,
        )],
    );
    let bytes = std::fs::read(&source).unwrap();
    let preview = manager
        .local_plan(&instance.id, vec![source.to_string_lossy().into()])
        .unwrap();
    assert!(preview.warnings.is_empty());
    assert!(!directory.join("mods/my-local.jar").exists());
    std::fs::write(&source, b"source changed after preview").unwrap();
    manager
        .local_finish(&preview.plan.token, false, false)
        .unwrap();
    assert_eq!(
        std::fs::read(directory.join("mods/my-local.jar")).unwrap(),
        bytes
    );
    assert_eq!(
        std::fs::read(&source).unwrap(),
        b"source changed after preview"
    );
    let restarted = ContentManager::new(manager.library.clone()).unwrap();
    let rows = restarted.installed(&instance.id).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter().filter(|r| r.record.provider == "local").count(),
        1
    );
    let local: Vec<_> = rows
        .into_iter()
        .filter(|r| r.record.provider == "local")
        .collect();
    restarted
        .change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Disable,
            files: selection(&local),
        })
        .unwrap();
    let disabled: Vec<_> = restarted
        .installed(&instance.id)
        .unwrap()
        .into_iter()
        .filter(|r| r.record.provider == "local")
        .collect();
    assert_eq!(disabled[0].status, "disabled");
    restarted
        .change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Delete,
            files: selection(&disabled),
        })
        .unwrap();
    assert_eq!(restarted.installed(&instance.id).unwrap().len(), 2);
}
#[test]
fn untracked_inventory_is_read_only_and_distinguishes_managed_incompatible_and_corrupt_files() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = installed_fixture(temp.path());
    local_jar(
        &directory.join("mods/manual.jar"),
        &[(
            "fabric.mod.json",
            r#"{"schemaVersion":1,"id":"manual","name":"Manual mod","version":"1.0","depends":{"minecraft":"1.21.1"}}"#,
        )],
    );
    local_jar(
        &directory.join("mods_disabled/old.jar"),
        &[(
            "fabric.mod.json",
            r#"{"schemaVersion":1,"id":"old","version":"1.0","depends":{"minecraft":"1.20.1"}}"#,
        )],
    );
    std::fs::write(directory.join("mods/broken.jar"), b"not a zip").unwrap();
    std::fs::write(directory.join("mods/notes.txt"), b"preserve me").unwrap();
    std::fs::write(directory.join("resourcepacks/manual.zip"), b"unknown pack").unwrap();
    std::fs::create_dir(directory.join("shaderpacks/unpacked")).unwrap();
    let receipt = std::fs::read(directory.join(".sporium/content.json")).unwrap();
    let rows = manager.untracked(&instance.id).unwrap();
    assert_eq!(rows.len(), 5);
    let manual = rows.iter().find(|r| r.filename == "manual.jar").unwrap();
    assert_eq!(manual.title, "Manual mod");
    assert_eq!(manual.metadata.as_ref().unwrap().mod_ids, ["manual"]);
    assert_eq!(manual.status, "unknown");
    let old = rows.iter().find(|r| r.filename == "old.jar").unwrap();
    assert!(old.disabled);
    assert_eq!(old.status, "incompatible");
    assert_eq!(
        rows.iter()
            .find(|r| r.filename == "broken.jar")
            .unwrap()
            .status,
        "unreadable"
    );
    assert_eq!(
        std::fs::read(directory.join(".sporium/content.json")).unwrap(),
        receipt
    );
    assert_eq!(manager.installed(&instance.id).unwrap().len(), 2);
    let restarted = ContentManager::new(manager.library.clone()).unwrap();
    assert_eq!(restarted.untracked(&instance.id).unwrap().len(), 5);
    std::fs::remove_file(directory.join("mods/manual.jar")).unwrap();
    assert_eq!(restarted.untracked(&instance.id).unwrap().len(), 4);
    assert_eq!(
        std::fs::read(directory.join("mods/notes.txt")).unwrap(),
        b"preserve me"
    );
}
#[test]
fn local_unknown_requires_explicit_acceptance_and_cancel_cleans_only_staging() {
    let temp = tempfile::tempdir().unwrap();
    let (library, instance) = library(temp.path());
    let manager = ContentManager::new(library).unwrap();
    let source = temp.path().join("unknown.jar");
    local_jar(&source, &[("readme.txt", "test")]);
    let p = manager
        .local_plan(&instance.id, vec![source.to_string_lossy().into()])
        .unwrap();
    assert!(!p.warnings.is_empty());
    assert!(matches!(
        manager.local_finish(&p.plan.token, false, false),
        Err(CoreError::ContentIncompatible)
    ));
    manager.local_finish(&p.plan.token, false, true).unwrap();
    assert!(source.exists());
    assert!(manager.installed(&instance.id).unwrap().is_empty());
    let p = manager
        .local_plan(&instance.id, vec![source.to_string_lossy().into()])
        .unwrap();
    manager.local_finish(&p.plan.token, true, false).unwrap();
    assert_eq!(
        manager.installed(&instance.id).unwrap()[0].record.provider,
        "local"
    );
}
#[test]
fn local_wrong_version_loader_side_duplicate_and_corrupt_archive_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let (library, instance) = library(temp.path());
    let manager = ContentManager::new(library).unwrap();
    let source = temp.path().join("test.jar");
    for metadata in [
        r#"{"schemaVersion":1,"id":"test","version":"1","depends":{"minecraft":"1.20.1"}}"#,
        r#"{"schemaVersion":1,"id":"test","version":"1","environment":"server","depends":{"minecraft":"*"}}"#,
    ] {
        local_jar(&source, &[("fabric.mod.json", metadata)]);
        assert!(matches!(
            manager.local_plan(&instance.id, vec![source.to_string_lossy().into()]),
            Err(CoreError::ContentIncompatible)
        ));
    }
    local_jar(
        &source,
        &[(
            "META-INF/mods.toml",
            "modLoader='javafml'\n[[mods]]\nmodId='test'\nversion='1'",
        )],
    );
    assert!(matches!(
        manager.local_plan(&instance.id, vec![source.to_string_lossy().into()]),
        Err(CoreError::ContentIncompatible)
    ));
    local_jar(
        &source,
        &[(
            "fabric.mod.json",
            r#"{"schemaVersion":1,"id":"test","version":"1","depends":{"minecraft":"*"}}"#,
        )],
    );
    let second = temp.path().join("different-name.jar");
    std::fs::copy(&source, &second).unwrap();
    assert!(matches!(
        manager.local_plan(
            &instance.id,
            vec![
                source.to_string_lossy().into(),
                second.to_string_lossy().into()
            ]
        ),
        Err(CoreError::ContentConflict)
    ));
    let p = manager
        .local_plan(&instance.id, vec![source.to_string_lossy().into()])
        .unwrap();
    manager.local_finish(&p.plan.token, false, false).unwrap();
    assert!(matches!(
        manager.local_plan(&instance.id, vec![second.to_string_lossy().into()]),
        Err(CoreError::ContentConflict)
    ));
    std::fs::write(&source, b"not a ZIP").unwrap();
    assert!(matches!(
        manager.local_plan(&instance.id, vec![source.to_string_lossy().into()]),
        Err(CoreError::Integrity)
    ));
}
#[test]
fn local_numeric_ranges_and_forge_neoforge_metadata_do_not_guess() {
    use local::matches_version as matches;
    assert_eq!(matches("1.21.1", ">=1.21 <1.22"), Some(true));
    assert_eq!(matches("1.20.1", "[1.21,1.22)"), Some(false));
    assert_eq!(matches("1.21.1", "1.21.x"), Some(true));
    assert_eq!(matches("1.21.1", "[1.21.1]"), Some(true));
    assert_eq!(matches("26w14a", ">=1.21"), None);
    assert_eq!(matches("1.21.1", "^1.21"), None);
    let temp = tempfile::tempdir().unwrap();
    let (_, mut instance) = library(temp.path());
    instance.loader = Loader::NeoForge;
    let source = temp.path().join("test.jar");
    local_jar(
        &source,
        &[(
            "META-INF/neoforge.mods.toml",
            "modLoader='javafml'\n[[mods]]\nmodId='test'\nversion='1'\n[[dependencies.test]]\nmodId='minecraft'\ntype='required'\nversionRange='[1.21,1.22)'\nside='BOTH'",
        )],
    );
    let (_, _, metadata) = local::inspect(&source, &instance).unwrap();
    assert_eq!(metadata.loader, "neoforge");
    assert!(!metadata.warnings.is_empty());
    instance.loader = Loader::Forge;
    assert!(matches!(
        local::inspect(&source, &instance),
        Err(CoreError::ContentIncompatible)
    ));
}

fn installed_fixture(root: &std::path::Path) -> (ContentManager, Instance, std::path::PathBuf) {
    let (library, instance) = library(root);
    let provider = fixture();
    let paths = Paths::new(root).unwrap();
    let directory = paths.location("instances", &instance.id).unwrap();
    let plan = resolve::resolve(&provider, &request(&instance), &instance, &[]).unwrap();
    for index in 0..plan.files.len() {
        let path = install::stage(&paths, &directory, &plan.token, index).unwrap();
        paths.mkdir(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"verified test content").unwrap();
    }
    install::commit(&paths, &directory, &plan).unwrap();
    (ContentManager::new(library).unwrap(), instance, directory)
}
fn selection(rows: &[InstalledContent]) -> Vec<ContentSelection> {
    rows.iter()
        .map(|r| ContentSelection {
            directory: r.record.directory.clone(),
            filename: r.record.file.filename.clone(),
            sha512: r.record.file.hashes.sha512.clone(),
        })
        .collect()
}
fn updating_fixture() -> Fixture {
    let mut provider = fixture();
    for (project, id, dep) in [
        ("parent", "parentNew", Some("depNew")),
        ("dependency", "depNew", None),
    ] {
        let mut v = version(project, id, None);
        v.version_number = "2.0".into();
        v.date_published = "2026-10-01".into();
        v.files[0].filename = format!("{project}-new.jar");
        let bytes = b"updated verified content";
        v.files[0].size = bytes.len() as u64;
        v.files[0].hashes.sha1 = hex(&sha1::Sha1::digest(bytes));
        v.files[0].hashes.sha512 = hex(&sha2::Sha512::digest(bytes));
        if let Some(dep) = dep {
            v.dependencies.push(ContentDependency {
                version_id: Some(dep.into()),
                project_id: Some("dependency".into()),
                file_name: None,
                dependency_type: "required".into(),
            });
        }
        provider.versions.push(v);
    }
    provider
}
#[test]
fn update_all_resolves_large_existing_libraries_as_one_dependency_consistent_plan() {
    let temp = tempfile::tempdir().unwrap();
    let (_, instance) = library(temp.path());
    let mut provider = fixture();
    let mut installed = vec![];
    for index in 0..100 {
        let id = format!("project{index}");
        let mut project = provider.projects["parent"].clone();
        project.id = id.clone();
        let old = version(&id, &format!("old{index}"), None);
        installed.extend(resolve::selected_files(&project, &old, false).unwrap());
        let mut new = old.clone();
        new.id = format!("new{index}");
        new.date_published = "2026-10-01".into();
        provider.versions.extend([old, new]);
        provider.projects.insert(id, project);
    }
    let selections: Vec<_> = installed
        .iter()
        .map(|r| ContentSelection {
            directory: r.directory.clone(),
            filename: r.file.filename.clone(),
            sha512: r.file.hashes.sha512.clone(),
        })
        .collect();
    let plan = update::plan(&provider, &instance, &installed, &[], &selections).unwrap();
    assert_eq!(plan.plan.files.len(), 100);
    assert_eq!(plan.previous.len(), 100);
    assert!(
        plan.plan
            .files
            .iter()
            .all(|r| r.version.id.starts_with("new"))
    );
}
fn stage_update(paths: &Paths, directory: &std::path::Path, plan: &ContentPlan) {
    for (index, _) in plan.files.iter().enumerate() {
        let file = install::stage(paths, directory, &plan.token, index).unwrap();
        paths.mkdir(file.parent().unwrap()).unwrap();
        std::fs::write(file, b"updated verified content").unwrap();
    }
}
#[test]
fn corrupt_update_artifact_blocks_entire_batch_and_same_name_replacement_is_verified() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = installed_fixture(temp.path());
    let paths = Paths::new(temp.path()).unwrap();
    let before = install::records(&paths, &directory).unwrap();
    let mut provider = updating_fixture();
    provider.versions[2].files[0].filename = "parent.jar".into();
    let plan = update::plan(
        &provider,
        &instance,
        &before,
        &[],
        &selection(&manager.installed(&instance.id).unwrap()),
    )
    .unwrap();
    stage_update(&paths, &directory, &plan.plan);
    std::fs::write(
        install::stage(&paths, &directory, &plan.plan.token, 1).unwrap(),
        b"corrupt",
    )
    .unwrap();
    assert!(matches!(
        update::commit(&paths, &directory, &instance, &plan.plan, &plan.previous),
        Err(CoreError::Integrity)
    ));
    assert_eq!(
        std::fs::read(directory.join("mods/parent.jar")).unwrap(),
        b"verified test content"
    );
    assert!(!directory.join(".sporium/content-update.json").exists());
    stage_update(&paths, &directory, &plan.plan);
    update::commit(&paths, &directory, &instance, &plan.plan, &plan.previous).unwrap();
    assert_eq!(
        std::fs::read(directory.join("mods/parent.jar")).unwrap(),
        b"updated verified content"
    );
    assert!(
        manager
            .installed(&instance.id)
            .unwrap()
            .iter()
            .all(|r| r.status == "installed")
    );
}
#[test]
fn update_check_uses_exact_compatibility_stable_channel_and_persistent_policy() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = installed_fixture(temp.path());
    let paths = Paths::new(temp.path()).unwrap();
    let installed = install::records(&paths, &directory).unwrap();
    let mut provider = updating_fixture();
    let mut alpha = provider.versions[2].clone();
    alpha.id = "alphaNew".into();
    alpha.version_type = "alpha".into();
    alpha.date_published = "2026-10-03".into();
    provider.versions.push(alpha);
    let mut wrong = provider.versions[2].clone();
    wrong.id = "wrongGame".into();
    wrong.game_versions = vec!["1.22".into()];
    wrong.date_published = "2026-10-04".into();
    provider.versions.push(wrong);
    let checked = update::check(&provider, &instance, &installed, &[]);
    assert_eq!(checked[0].candidate.as_ref().unwrap().id, "parentNew");
    let policy = ContentUpdatePolicy {
        project_id: "parent".into(),
        pinned: true,
        ignored_version: None,
    };
    manager.update_policy(&instance.id, policy).unwrap();
    let policies = update::policies(&paths, &directory).unwrap();
    assert_eq!(
        update::check(&provider, &instance, &installed, &policies)[0].status,
        "pinned"
    );
    manager
        .update_policy(
            &instance.id,
            ContentUpdatePolicy {
                project_id: "parent".into(),
                pinned: false,
                ignored_version: Some("parentNew".into()),
            },
        )
        .unwrap();
    let restarted = update::policies(&paths, &directory).unwrap();
    assert_eq!(
        update::check(&provider, &instance, &installed, &restarted)[0].status,
        "ignored"
    );
    provider.versions.retain(|v| v.id != "parentNew");
    assert_eq!(
        update::check(&provider, &instance, &installed, &[])[0].status,
        "incompatible"
    );
    provider.projects.remove("parent");
    assert_eq!(
        update::check(&provider, &instance, &installed, &[])[0].status,
        "unavailable"
    );
}
#[test]
fn updating_required_dependency_honors_pins_and_retained_exact_requirements() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = installed_fixture(temp.path());
    let paths = Paths::new(temp.path()).unwrap();
    let installed = install::records(&paths, &directory).unwrap();
    let parent = selection(&manager.installed(&instance.id).unwrap()[..1]);
    let provider = updating_fixture();
    let plan = update::plan(&provider, &instance, &installed, &[], &parent).unwrap();
    assert_eq!(plan.plan.files.len(), 2);
    let pinned = [ContentUpdatePolicy {
        project_id: "dependency".into(),
        pinned: true,
        ignored_version: None,
    }];
    assert!(matches!(
        update::plan(&provider, &instance, &installed, &pinned, &parent),
        Err(CoreError::DependencyConflict)
    ));
    let mut retained = installed.clone();
    retained[0].version.dependencies[0].version_id = Some("depVersion".into());
    let dep = selection(&manager.installed(&instance.id).unwrap()[1..]);
    assert!(matches!(
        update::plan(&provider, &instance, &retained, &[], &dep),
        Err(CoreError::DependencyConflict)
    ));
}
#[test]
fn content_update_preserves_disabled_state_provenance_worlds_and_snapshot_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = installed_fixture(temp.path());
    let rows = manager.installed(&instance.id).unwrap();
    manager
        .change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Disable,
            files: selection(&rows[..1]),
        })
        .unwrap();
    let paths = Paths::new(temp.path()).unwrap();
    let before = install::records(&paths, &directory).unwrap();
    std::fs::write(directory.join("mods/manual.jar"), b"manual").unwrap();
    std::fs::write(directory.join("saves/world.dat"), b"world").unwrap();
    paths.mkdir(&directory.join("config")).unwrap();
    std::fs::write(directory.join("config/settings.toml"), b"settings").unwrap();
    let rows = manager.installed(&instance.id).unwrap();
    let plan = update::plan(
        &updating_fixture(),
        &instance,
        &before,
        &[],
        &selection(&rows[..1]),
    )
    .unwrap();
    assert_eq!(plan.plan.files[0].directory, "mods_disabled");
    stage_update(&paths, &directory, &plan.plan);
    update::commit(&paths, &directory, &instance, &plan.plan, &plan.previous).unwrap();
    let restarted = manager.installed(&instance.id).unwrap();
    assert_eq!(restarted.len(), 2);
    assert_eq!(restarted[0].status, "disabled");
    assert_eq!(restarted[0].record.version.id, "parentNew");
    assert!(!directory.join("mods_disabled/parent.jar").exists());
    assert_eq!(
        std::fs::read(directory.join("mods/manual.jar")).unwrap(),
        b"manual"
    );
    assert_eq!(
        std::fs::read(directory.join("saves/world.dat")).unwrap(),
        b"world"
    );
    let snapshot = directory
        .join(".sporium/content-snapshots")
        .join(&plan.plan.token);
    assert_eq!(
        std::fs::read(snapshot.join("files/0.bin")).unwrap(),
        b"verified test content"
    );
    assert_eq!(
        std::fs::read(snapshot.join("settings/config/settings.toml")).unwrap(),
        b"settings"
    );
    assert!(!snapshot.join("settings/saves").exists());
    assert!(
        manager
            .history(&instance.id)
            .unwrap()
            .last()
            .unwrap()
            .titles[0]
            .contains("1.0 → 2.0")
    );
}

fn rollback_fixture(
    root: &std::path::Path,
) -> (ContentManager, Instance, std::path::PathBuf, String) {
    let (manager, instance, directory) = installed_fixture(root);
    let paths = Paths::new(root).unwrap();
    let before = install::records(&paths, &directory).unwrap();
    let plan = update::plan(
        &updating_fixture(),
        &instance,
        &before,
        &[],
        &selection(&manager.installed(&instance.id).unwrap()),
    )
    .unwrap();
    stage_update(&paths, &directory, &plan.plan);
    update::commit(&paths, &directory, &instance, &plan.plan, &plan.previous).unwrap();
    (manager, instance, directory, plan.plan.token)
}

#[test]
fn restore_points_restore_exact_receipts_and_allow_undo_without_touching_worlds_or_user_files() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory, point) = rollback_fixture(temp.path());
    std::fs::write(directory.join("mods/manual.jar"), b"manual").unwrap();
    std::fs::write(directory.join("saves/world.dat"), b"world").unwrap();
    std::fs::write(directory.join("options.txt"), b"changed settings").unwrap();
    assert!(manager.restore_points(&instance.id).unwrap()[0].available);
    manager.restore_content(&instance.id, &point).unwrap();
    assert!(
        manager
            .installed(&instance.id)
            .unwrap()
            .iter()
            .all(|r| r.record.version.version_number == "1.0")
    );
    assert!(!directory.join("mods/parent-new.jar").exists());
    let undo = manager
        .restore_points(&instance.id)
        .unwrap()
        .into_iter()
        .find(|p| p.id != point)
        .unwrap();
    manager.restore_content(&instance.id, &undo.id).unwrap();
    assert!(
        manager
            .installed(&instance.id)
            .unwrap()
            .iter()
            .all(|r| r.record.version.version_number == "2.0")
    );
    assert_eq!(
        std::fs::read(directory.join("mods/manual.jar")).unwrap(),
        b"manual"
    );
    assert_eq!(
        std::fs::read(directory.join("saves/world.dat")).unwrap(),
        b"world"
    );
    assert_eq!(
        std::fs::read(directory.join("options.txt")).unwrap(),
        b"changed settings"
    );
    assert_eq!(
        manager
            .history(&instance.id)
            .unwrap()
            .iter()
            .filter(|e| e.action == "restore")
            .count(),
        2
    );
}

#[test]
fn rollback_refuses_corrupt_snapshots_modified_content_collisions_and_active_game_without_mutation()
{
    for scenario in [
        "corrupt",
        "modified",
        "collision",
        "identical_collision",
        "busy",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (manager, instance, directory, point) = rollback_fixture(temp.path());
        let paths = Paths::new(temp.path()).unwrap();
        let before = install::records(&paths, &directory).unwrap();
        let lease = if scenario == "busy" {
            Some(manager.library.lease_game(&instance.id).unwrap().2)
        } else {
            None
        };
        match scenario {
            "corrupt" => std::fs::write(
                update::snapshot_file(&paths, &directory, &point, 0).unwrap(),
                b"corrupt",
            )
            .unwrap(),
            "modified" => {
                std::fs::write(directory.join("mods/parent-new.jar"), b"changed").unwrap()
            }
            "collision" => {
                std::fs::write(directory.join("mods/parent.jar"), b"unknown file").unwrap()
            }
            "identical_collision" => {
                std::fs::write(directory.join("mods/parent.jar"), b"verified test content")
                    .unwrap();
            }
            _ => (),
        }
        assert!(
            manager.restore_content(&instance.id, &point).is_err(),
            "{scenario}"
        );
        assert_eq!(
            serde_json::to_vec(&before).unwrap(),
            serde_json::to_vec(&install::records(&paths, &directory).unwrap()).unwrap()
        );
        assert!(!directory.join(".sporium/content-restore.json").exists());
        drop(lease);
        if scenario == "corrupt" {
            assert!(!manager.restore_points(&instance.id).unwrap()[0].available);
        }
    }
}

#[test]
fn interrupted_rollback_recovers_before_receipt_publication_and_history_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory, source) = rollback_fixture(temp.path());
    let paths = Paths::new(temp.path()).unwrap();
    let before = install::records(&paths, &directory).unwrap();
    manager.restore_content(&instance.id, &source).unwrap();
    let after = install::records(&paths, &directory).unwrap();
    let token = manager
        .restore_points(&instance.id)
        .unwrap()
        .into_iter()
        .find(|p| p.id != source)
        .unwrap()
        .id;
    install::write_records(&paths, &directory, &before).unwrap();
    std::fs::remove_file(directory.join("mods/parent.jar")).unwrap();
    std::fs::write(directory.join(".sporium/content-restore.json"), serde_json::to_vec(&serde_json::json!({"schema":1,"token":token,"source":source,"before":before,"after":after,"event":{"id":token,"timestamp":1,"action":"restore","titles":["restored"]}})).unwrap()).unwrap();
    let recovered = manager.installed(&instance.id).unwrap();
    assert!(
        recovered
            .iter()
            .all(|r| r.record.version.version_number == "1.0" && r.status == "installed")
    );
    install::recover(&paths, &directory).unwrap();
    assert!(!directory.join(".sporium/content-restore.json").exists());
    assert_eq!(
        manager
            .history(&instance.id)
            .unwrap()
            .iter()
            .filter(|e| e.action == "restore")
            .count(),
        1
    );
}
#[test]
fn update_preflight_blocks_modified_sources_unknown_destinations_and_stale_plans() {
    let temp = tempfile::tempdir().unwrap();
    let (mut manager, instance, directory) = installed_fixture(temp.path());
    manager.provider = Arc::new(updating_fixture());
    let rows = manager.installed(&instance.id).unwrap();
    let request = || ContentUpdateRequest {
        instance_id: instance.id.clone(),
        files: selection(&rows),
    };
    let plan = manager.update_plan(request()).unwrap();
    std::fs::write(directory.join("mods/dependency.jar"), b"modified").unwrap();
    assert!(matches!(
        manager.start(&plan.plan.token),
        Err(CoreError::ContentConflict)
    ));
    assert_eq!(
        std::fs::read(directory.join("mods/parent.jar")).unwrap(),
        b"verified test content"
    );
    std::fs::write(
        directory.join("mods/dependency.jar"),
        b"verified test content",
    )
    .unwrap();
    std::fs::write(directory.join("mods/parent-new.jar"), b"unknown").unwrap();
    assert!(matches!(
        manager.update_plan(request()),
        Err(CoreError::ContentConflict)
    ));
    std::fs::remove_file(directory.join("mods/parent-new.jar")).unwrap();
    manager
        .update_policy(
            &instance.id,
            ContentUpdatePolicy {
                project_id: "parent".into(),
                pinned: true,
                ignored_version: None,
            },
        )
        .unwrap();
    assert!(matches!(
        manager.start(&plan.plan.token),
        Err(CoreError::RecordConflict)
    ));
    let (_, _, lease) = manager.library.lease_game(&instance.id).unwrap();
    assert!(matches!(
        manager.update_plan(request()),
        Err(CoreError::InstanceBusy)
    ));
    drop(lease);
}
#[test]
fn interrupted_update_recovers_full_batch_before_reads_and_launch_idempotently() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = installed_fixture(temp.path());
    let paths = Paths::new(temp.path()).unwrap();
    let before = install::records(&paths, &directory).unwrap();
    let plan = update::plan(
        &updating_fixture(),
        &instance,
        &before,
        &[],
        &selection(&manager.installed(&instance.id).unwrap()),
    )
    .unwrap();
    stage_update(&paths, &directory, &plan.plan);
    let root = directory
        .join(".sporium/content-snapshots")
        .join(&plan.plan.token)
        .join("files");
    paths.mkdir(&root).unwrap();
    for (index, _) in before.iter().enumerate() {
        std::fs::write(root.join(format!("{index}.bin")), b"verified test content").unwrap();
    }
    std::fs::write(
        root.parent().unwrap().join("snapshot.json"),
        serde_json::to_vec(
            &serde_json::json!({"schema":1,"instance":instance,"files":before,"settings":[]}),
        )
        .unwrap(),
    )
    .unwrap();
    std::fs::write(directory.join(".sporium/content-update.json"), serde_json::to_vec(&serde_json::json!({ "schema":1, "token":plan.plan.token, "before":before, "after":plan.plan.files, "previous":before, "files":plan.plan.files, "event":{"id":plan.plan.token,"timestamp":1,"action":"update","titles":["parent: 1 → 2"]} })).unwrap()).unwrap();
    std::fs::remove_file(directory.join("mods/parent.jar")).unwrap();
    std::fs::write(
        directory.join("mods/parent-new.jar"),
        b"updated verified content",
    )
    .unwrap();
    let recovered = manager.installed(&instance.id).unwrap();
    assert!(
        recovered
            .iter()
            .all(|r| r.status == "installed" && r.record.version.version_number == "2.0")
    );
    assert!(!directory.join("mods/dependency.jar").exists());
    let (_, _, lease) = manager.library.lease_game(&instance.id).unwrap();
    drop(lease);
    assert_eq!(
        manager
            .history(&instance.id)
            .unwrap()
            .iter()
            .filter(|e| e.action == "update")
            .count(),
        1
    );
    assert!(!directory.join(".sporium/content-update.json").exists());
}
#[test]
fn content_bulk_toggle_delete_survives_restart_with_provenance_and_history() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = installed_fixture(temp.path());
    let initial = manager.installed(&instance.id).unwrap();
    std::fs::write(directory.join("mods/local.jar"), b"local data").unwrap();
    manager
        .change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Disable,
            files: selection(&initial),
        })
        .unwrap();
    let restarted = ContentManager::new(manager.library.clone()).unwrap();
    let disabled = restarted.installed(&instance.id).unwrap();
    assert!(
        disabled
            .iter()
            .all(|r| r.status == "disabled" && r.record.provider == "modrinth")
    );
    assert!(!directory.join("mods/parent.jar").exists());
    assert!(directory.join("mods_disabled/parent.jar").exists());
    restarted
        .change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Enable,
            files: selection(&disabled),
        })
        .unwrap();
    let enabled = restarted.installed(&instance.id).unwrap();
    assert!(enabled.iter().all(|r| r.status == "installed"));
    assert_eq!(
        serde_json::to_value(&initial).unwrap(),
        serde_json::to_value(&enabled).unwrap()
    );
    restarted
        .change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Delete,
            files: selection(&enabled),
        })
        .unwrap();
    assert!(restarted.installed(&instance.id).unwrap().is_empty());
    assert_eq!(
        std::fs::read(directory.join("mods/local.jar")).unwrap(),
        b"local data"
    );
    assert_eq!(
        restarted
            .history(&instance.id)
            .unwrap()
            .iter()
            .map(|e| e.action.as_str())
            .collect::<Vec<_>>(),
        vec!["add", "disable", "enable", "delete"]
    );
}
#[test]
fn changed_files_stale_selection_and_destination_collision_preserve_entire_batch() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = installed_fixture(temp.path());
    let rows = manager.installed(&instance.id).unwrap();
    std::fs::write(directory.join("mods/dependency.jar"), b"user edit").unwrap();
    assert!(matches!(
        manager.change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Disable,
            files: selection(&rows)
        }),
        Err(CoreError::ContentConflict)
    ));
    assert!(directory.join("mods/parent.jar").exists());
    assert!(!directory.join(".sporium/content-change.json").exists());
    std::fs::write(
        directory.join("mods/dependency.jar"),
        b"verified test content",
    )
    .unwrap();
    std::fs::create_dir_all(directory.join("mods_disabled")).unwrap();
    std::fs::write(directory.join("mods_disabled/parent.jar"), b"another mod").unwrap();
    assert!(matches!(
        manager.change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Disable,
            files: selection(&rows)
        }),
        Err(CoreError::ContentConflict)
    ));
    let mut stale = selection(&rows);
    stale[0].sha512 = "0".repeat(128);
    assert!(matches!(
        manager.change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Delete,
            files: stale
        }),
        Err(CoreError::RecordConflict)
    ));
    let (_, _, _lease) = manager.library.lease_game(&instance.id).unwrap();
    assert!(matches!(
        manager.change(ContentChange {
            instance_id: instance.id.clone(),
            action: ContentAction::Delete,
            files: selection(&rows)
        }),
        Err(CoreError::InstanceBusy)
    ));
}
#[test]
fn interrupted_bulk_disable_rolls_forward_before_read_or_launch_without_duplicate_history() {
    let temp = tempfile::tempdir().unwrap();
    let (manager, instance, directory) = installed_fixture(temp.path());
    let rows = manager.installed(&instance.id).unwrap();
    let before: Vec<_> = rows.iter().map(|r| r.record.clone()).collect();
    let mut after = before.clone();
    for r in &mut after {
        r.directory = "mods_disabled".into();
    }
    let token = uuid::Uuid::new_v4().to_string();
    std::fs::write(directory.join(".sporium/content-change.json"), serde_json::to_vec(&serde_json::json!({
        "schema":1,"token":token,"action":"disable","before":before,"after":after,"selected":before,
        "event":{"id":token,"timestamp":1,"action":"disable","titles":["parent","dependency"]}
    })).unwrap()).unwrap();
    std::fs::create_dir_all(directory.join("mods_disabled")).unwrap();
    std::fs::copy(
        directory.join("mods/parent.jar"),
        directory.join("mods_disabled/parent.jar"),
    )
    .unwrap();
    let recovered = manager.installed(&instance.id).unwrap();
    assert!(recovered.iter().all(|r| r.status == "disabled"));
    assert!(!directory.join("mods/parent.jar").exists());
    assert!(!directory.join("mods/dependency.jar").exists());
    let (_, _, _lease) = manager.library.lease_game(&instance.id).unwrap();
    drop(_lease);
    assert_eq!(manager.history(&instance.id).unwrap().len(), 2);
}
#[test]
fn plugin_versions_require_exact_server_platform_and_never_install_in_client_instances() {
    let temp = tempfile::tempdir().unwrap();
    let (_, instance) = library(temp.path());
    let mut provider = fixture();
    let v = &mut provider.versions[0];
    v.loaders = vec!["paper".into()];
    v.environment = "dedicated_server_only".into();
    assert_eq!(
        provider
            .plugin_versions("parent", "1.21.1", "paper")
            .unwrap()
            .len(),
        1
    );
    assert!(
        provider
            .plugin_versions("parent", "1.21.1", "purpur")
            .unwrap()
            .is_empty()
    );
    assert!(
        provider
            .plugin_versions("parent", "1.20.1", "paper")
            .unwrap()
            .is_empty()
    );
    assert!(
        provider
            .plugin_versions("parent", "1.21.1", "fabric")
            .is_err()
    );
    assert!(!resolve::compatible(
        &provider.projects["parent"],
        &provider.versions[0],
        &instance
    ));
    let facets = modrinth::facets(&CatalogQuery {
        kind: "plugin".into(),
        ..Default::default()
    })
    .unwrap();
    assert!(facets.contains("all_project_types:plugin"));
    assert!(!facets.contains("project_type:mod"));
    provider.versions[0].environment = "client_only".into();
    assert!(
        provider
            .plugin_versions("parent", "1.21.1", "paper")
            .unwrap()
            .is_empty()
    );
}
#[test]
#[ignore = "Live official Modrinth API; run explicitly when validating provider changes"]
fn live_plugin_discovery_and_version_selection() {
    let temp = tempfile::tempdir().unwrap();
    let provider = modrinth::Modrinth::new(temp.path().into()).unwrap();
    let hits = provider
        .search(&CatalogQuery {
            query: "spark".into(),
            kind: "plugin".into(),
            minecraft: "1.21.1".into(),
            loader: "paper".into(),
            sort: "downloads".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(!hits.hits.is_empty());
    let mut found = false;
    for hit in hits.hits.iter().take(3) {
        let versions = provider
            .plugin_versions(&hit.id, "1.21.1", "paper")
            .unwrap();
        if !versions.is_empty() {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "Search hits must have a real compatible downloadable plugin version"
    );
}
#[test]
fn dependencies_are_compatible_deduplicated_and_optional_not_installed() {
    let temp = tempfile::tempdir().unwrap();
    let (_, instance) = library(temp.path());
    let mut fixture = fixture();
    fixture.versions[1].dependencies.push(ContentDependency {
        version_id: Some("parentVersion".into()),
        project_id: Some("parent".into()),
        file_name: None,
        dependency_type: "required".into(),
    });
    fixture.versions[0].dependencies.push(ContentDependency {
        version_id: None,
        project_id: Some("optional".into()),
        file_name: None,
        dependency_type: "optional".into(),
    });
    let plan = resolve::resolve(&fixture, &request(&instance), &instance, &[]).unwrap();
    assert_eq!(plan.files.len(), 2);
    assert_eq!(plan.optional_dependencies, 1);
    assert!(
        plan.files
            .iter()
            .any(|r| r.project_id == "dependency" && r.dependency)
    );
    fixture.versions[1].loaders = vec!["forge".into()];
    assert!(resolve::resolve(&fixture, &request(&instance), &instance, &[]).is_err());
}
#[test]
fn exact_dependency_and_incompatible_installed_versions_are_enforced() {
    let temp = tempfile::tempdir().unwrap();
    let (_, instance) = library(temp.path());
    let mut fixture = fixture();
    fixture.versions[0].dependencies[0].version_id = Some("depVersion".into());
    let plan = resolve::resolve(&fixture, &request(&instance), &instance, &[]).unwrap();
    let mut installed = vec![plan.files[1].clone()];
    installed[0].version.id = "oldVersion".into();
    assert!(matches!(
        resolve::resolve(&fixture, &request(&instance), &instance, &installed),
        Err(CoreError::ContentConflict)
    ));
    fixture.versions[0].dependencies[0].dependency_type = "incompatible".into();
    assert!(matches!(
        resolve::resolve(&fixture, &request(&instance), &instance, &plan.files[1..]),
        Err(CoreError::DependencyConflict)
    ));
}
#[test]
fn unsafe_files_and_dedicated_server_or_wrong_loader_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let (_, mut instance) = library(temp.path());
    let fixture = fixture();
    let project = &fixture.projects["parent"];
    let mut v = fixture.versions[0].clone();
    assert!(resolve::compatible(project, &v, &instance));
    instance.loader = Loader::NeoForge;
    assert!(!resolve::compatible(project, &v, &instance));
    instance.loader = Loader::Fabric;
    v.environment = "dedicated_server_only".into();
    assert!(!resolve::compatible(project, &v, &instance));
    for name in [
        "../evil.jar",
        "CON.jar",
        "sub/evil.jar",
        "bad.jar:stream",
        "payload.exe",
    ] {
        let mut file = v.files[0].clone();
        file.filename = name.into();
        assert!(resolve::validate_file(&file, "mods").is_err());
    }
    let mut file = v.files[0].clone();
    file.url = "https://evil.example/mod.jar".into();
    assert!(resolve::validate_file(&file, "mods").is_err());
}
#[test]
fn content_commit_is_idempotent_preserves_local_files_and_recovers_after_partial_publish() {
    let temp = tempfile::tempdir().unwrap();
    let (library, instance) = library(temp.path());
    let fixture = fixture();
    let paths = Paths::new(temp.path()).unwrap();
    let directory = paths.location("instances", &instance.id).unwrap();
    let plan = resolve::resolve(&fixture, &request(&instance), &instance, &[]).unwrap();
    for (index, _) in plan.files.iter().enumerate() {
        let stage = install::stage(&paths, &directory, &plan.token, index).unwrap();
        paths.mkdir(stage.parent().unwrap()).unwrap();
        std::fs::write(stage, b"verified test content").unwrap();
    }
    let target = install::target(&paths, &directory, &plan.files[0]).unwrap();
    std::fs::write(&target, b"local mod").unwrap();
    assert!(matches!(
        install::commit(&paths, &directory, &plan),
        Err(CoreError::ContentConflict)
    ));
    assert_eq!(std::fs::read(&target).unwrap(), b"local mod");
    paths.remove(&target).unwrap();
    let sentinel = directory.join("saves/world.dat");
    std::fs::write(&sentinel, b"world").unwrap();
    // Simulate power loss after one atomic publish but before the receipt update.
    let journal = serde_json::json!({"schema":1,"token":plan.token,"files":plan.files});
    crate::game::fs::write_atomic(
        &paths,
        &directory.join(".sporium/content-pending.json"),
        &serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();
    std::fs::write(&target, b"verified test content").unwrap();
    let (_, _, lease) = library.lease_game(&instance.id).unwrap();
    drop(lease);
    assert_eq!(install::records(&paths, &directory).unwrap().len(), 2);
    install::recover(&paths, &directory).unwrap();
    assert_eq!(std::fs::read(sentinel).unwrap(), b"world");
    assert!(!directory.join(".sporium/content-pending.json").exists());
    std::fs::write(&target, b"user modification").unwrap();
    assert!(matches!(
        install::preflight(
            &paths,
            &directory,
            &plan.files,
            &install::records(&paths, &directory).unwrap()
        ),
        Err(CoreError::ContentConflict)
    ));
}
#[test]
fn facets_keep_cross_filter_and_and_within_environment_or() {
    let query = CatalogQuery {
        kind: "mod".into(),
        minecraft: "1.21.1".into(),
        loader: "fabric".into(),
        environment: "both".into(),
        ..Default::default()
    };
    let facets: Vec<Vec<String>> =
        serde_json::from_str(&modrinth::facets(&query).unwrap()).unwrap();
    assert!(facets.contains(&vec!["versions:1.21.1".into()]));
    assert!(facets.contains(&vec!["categories:fabric".into()]));
    assert!(
        facets
            .iter()
            .any(|g| g.len() == 3 && g.contains(&"environment:client_and_server".into()))
    );
}

#[test]
fn prepared_plan_cannot_ignore_newly_installed_content() {
    let temp = tempfile::tempdir().unwrap();
    let (library, instance) = library(temp.path());
    let fixture = fixture();
    let manager = ContentManager {
        automatic: automatic::runtime(),
        library: library.clone(),
        provider: Arc::new(fixture),
        state: Arc::new(Mutex::new(State::default())),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let plan = manager.plan(request(&instance)).unwrap();
    let paths = Paths::new(temp.path()).unwrap();
    let directory = paths.location("instances", &instance.id).unwrap();
    crate::game::fs::write_atomic(
        &paths,
        &directory.join(".sporium/content.json"),
        &serde_json::to_vec(&serde_json::json!({"schema":1,"files":[plan.files[1]]})).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        manager.start(&plan.token),
        Err(CoreError::RecordConflict)
    ));
    assert!(!manager.is_active());
    assert!(
        !install::target(&paths, &directory, &plan.files[0])
            .unwrap()
            .exists()
    );
}

#[test]
fn new_instance_plan_is_read_only_validates_dependencies_and_preserves_icons() {
    let temp = tempfile::tempdir().unwrap();
    let (library, _) = library(temp.path());
    let mut fixture = fixture();
    fixture.projects.get_mut("parent").unwrap().icon_url =
        Some("https://cdn.modrinth.com/data/parent/icon.png".into());
    let manager = ContentManager {
        automatic: automatic::runtime(),
        library: library.clone(),
        provider: Arc::new(fixture),
        state: Arc::new(Mutex::new(State::default())),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let before = serde_json::to_vec(&library.snapshot().unwrap()).unwrap();
    let mut request = ContentCreateRequest {
        project_id: "parent".into(),
        version_id: "parentVersion".into(),
        instance: CreateInstance {
            name: "New from mod".into(),
            minecraft_version: "1.21.1".into(),
            loader: Loader::Fabric,
            collection_id: None,
        },
    };
    let plan = manager.create_plan(request.clone()).unwrap();
    assert_eq!(plan.files.len(), 2);
    assert!(plan.new_instance.is_some());
    assert!(plan.files.iter().any(|f| f.icon_url.is_some()));
    assert_eq!(
        before,
        serde_json::to_vec(&library.snapshot().unwrap()).unwrap()
    );
    request.instance.loader = Loader::Forge;
    assert!(matches!(
        manager.create_plan(request),
        Err(CoreError::ContentIncompatible)
    ));
    assert_eq!(
        before,
        serde_json::to_vec(&library.snapshot().unwrap()).unwrap()
    );
    // A plain content worker cannot silently create an instance without preparing its game.
    assert!(matches!(
        manager.start(&plan.token),
        Err(CoreError::InvalidInput)
    ));
    assert_eq!(
        before,
        serde_json::to_vec(&library.snapshot().unwrap()).unwrap()
    );
    let change = library
        .create_with_icon(plan.new_instance.unwrap(), plan.files[0].icon_url.clone())
        .unwrap();
    let instance = change
        .snapshot
        .instances
        .iter()
        .find(|i| i.id == change.affected_id)
        .unwrap();
    assert_eq!(instance.minecraft_version, "1.21.1");
    assert_eq!(instance.loader, Loader::Fabric);
    assert!(instance.icon_ref.is_some());
}

#[test]
fn old_content_receipts_without_icons_remain_readable() {
    let temp = tempfile::tempdir().unwrap();
    let (_, instance) = library(temp.path());
    let plan = resolve::resolve(&fixture(), &request(&instance), &instance, &[]).unwrap();
    let mut record = serde_json::to_value(&plan.files[0]).unwrap();
    record.as_object_mut().unwrap().remove("iconUrl");
    assert!(
        serde_json::from_value::<ContentRecord>(record)
            .unwrap()
            .icon_url
            .is_none()
    );
}

#[test]
fn cancellation_before_game_preparation_preserves_instance_and_does_not_launch() {
    let temp = tempfile::tempdir().unwrap();
    let (library, instance) = library(temp.path());
    let database = Database::new(temp.path().join("launcher/sporium.sqlite3"));
    let game = crate::game::GameManager::new(library.clone(), database);
    let (_, directory, _lease) = library.lease_game(&instance.id).unwrap();
    let cancelled = Arc::new(AtomicBool::new(true));
    assert!(matches!(
        game.prepare_content_base(&instance, &directory, cancelled),
        Err(CoreError::Cancelled)
    ));
    let state = game.snapshot();
    assert!(matches!(
        state.job.unwrap().phase,
        crate::game::model::JobPhase::Cancelled
    ));
    assert!(state.sessions.is_empty());
    assert!(
        !temp
            .path()
            .join("launcher/download-operation.json")
            .exists()
    );
    assert!(directory.exists());
    assert_eq!(library.snapshot().unwrap().instances.len(), 1);
}

#[test]
fn resource_and_shader_destinations_require_supported_files_and_exact_game_version() {
    let temp = tempfile::tempdir().unwrap();
    let (_, instance) = library(temp.path());
    let fixture = fixture();
    for (kind, loader, directory) in [
        ("resourcepack", "minecraft", "resourcepacks"),
        ("shader", "iris", "shaderpacks"),
    ] {
        let mut project = fixture.projects["parent"].clone();
        project.project_type = kind.into();
        let mut version = fixture.versions[0].clone();
        version.loaders = vec![loader.into()];
        version.files[0].filename = "pack.zip".into();
        version.environment = "unknown".into();
        assert!(resolve::compatible(&project, &version, &instance));
        assert_eq!(
            resolve::selected_files(&project, &version, false).unwrap()[0].directory,
            directory
        );
        version.game_versions = vec!["1.20.1".into()];
        assert!(!resolve::compatible(&project, &version, &instance));
    }
}

#[test]
fn corrupt_staging_never_publishes_even_one_file() {
    let temp = tempfile::tempdir().unwrap();
    let (_, instance) = library(temp.path());
    let fixture = fixture();
    let paths = Paths::new(temp.path()).unwrap();
    let directory = paths.location("instances", &instance.id).unwrap();
    let plan = resolve::resolve(&fixture, &request(&instance), &instance, &[]).unwrap();
    for (index, _) in plan.files.iter().enumerate() {
        let staged = install::stage(&paths, &directory, &plan.token, index).unwrap();
        paths.mkdir(staged.parent().unwrap()).unwrap();
        std::fs::write(
            staged,
            if index == 0 {
                b"verified test content".as_slice()
            } else {
                b"corrupt".as_slice()
            },
        )
        .unwrap();
    }
    assert!(matches!(
        install::commit(&paths, &directory, &plan),
        Err(CoreError::Integrity)
    ));
    for record in &plan.files {
        assert!(
            !install::target(&paths, &directory, record)
                .unwrap()
                .exists()
        );
    }
    assert!(directory.join(".sporium/content-pending.json").exists());
}

#[test]
fn content_status_readers_can_coexist_but_block_game_or_mutations() {
    let temp = tempfile::tempdir().unwrap();
    let (library, instance) = library(temp.path());
    let (_, first) = library.lease_content_read(&instance.id).unwrap();
    let (_, second) = library.lease_content_read(&instance.id).unwrap();
    assert!(matches!(
        library.lease_game(&instance.id),
        Err(CoreError::InstanceBusy)
    ));
    drop(first);
    drop(second);
    assert!(library.lease_game(&instance.id).is_ok());
}
