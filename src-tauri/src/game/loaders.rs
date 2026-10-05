use super::{
    GameManager,
    fs::{java_path, read_limited, relative, write_atomic},
    java,
    job_object::ProcessGroup,
    metadata::{self, string},
    model::{JavaRuntime, JobPhase},
    network::{Hash, Network, hex, verify},
};
use crate::{
    error::CoreError,
    instances::{
        filesystem::Paths,
        model::{Instance, Loader},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha1::Digest;
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersion {
    pub id: String,
    pub stable: bool,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LoaderCatalog {
    pub versions: Vec<LoaderVersion>,
    pub cached: bool,
}
pub fn valid_version(value: &str) -> Result<(), CoreError> {
    if value.is_empty()
        || value.len() > 100
        || value.contains("..")
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_+".contains(&b))
    {
        return Err(CoreError::InvalidInput);
    }
    Ok(())
}
pub fn key(loader: Loader) -> &'static str {
    match loader {
        Loader::Vanilla => "vanilla",
        Loader::Fabric => "fabric",
        Loader::Forge => "forge",
        Loader::NeoForge => "neoforge",
    }
}
fn cached_bytes(
    paths: &Paths,
    network: &Network,
    url: &str,
    limit: u64,
) -> Result<(Vec<u8>, bool), CoreError> {
    let cache = paths
        .root()
        .join("shared/cache/loaders")
        .join(format!("{}.json", hex(&sha1::Sha1::digest(url.as_bytes()))));
    let old = read_limited(paths, &cache, limit).ok();
    // Versioned Maven checksums are immutable; do not require a network round trip
    // on each offline launch just because a catalog's short TTL elapsed.
    if (url.ends_with(".sha1") || url.ends_with("/profile/json"))
        && let Some(bytes) = &old
    {
        return Ok((bytes.clone(), true));
    }
    if cache
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| m.elapsed().ok())
        .is_some_and(|age| age < Duration::from_secs(900))
        && let Some(bytes) = &old
    {
        return Ok((bytes.clone(), true));
    }
    match network.bytes(url, limit) {
        Ok(bytes) => {
            write_atomic(paths, &cache, &bytes)?;
            Ok((bytes, false))
        }
        Err(error) => old.map(|b| (b, true)).ok_or(error),
    }
}
pub fn catalog(root: &Path, loader: Loader, minecraft: &str) -> Result<LoaderCatalog, CoreError> {
    crate::instances::model::game_version(minecraft)?;
    let paths = Paths::new(root)?;
    let network = Network::new()?;
    if loader == Loader::Vanilla {
        return Ok(LoaderCatalog {
            versions: vec![],
            cached: false,
        });
    }
    let url = match loader {
        Loader::Fabric => {
            let mut u =
                reqwest::Url::parse("https://meta.fabricmc.net/v2/versions/loader/").unwrap();
            u.path_segments_mut().unwrap().push(minecraft);
            u.to_string()
        }
        Loader::Forge => {
            "https://files.minecraftforge.net/net/minecraftforge/forge/maven-metadata.json".into()
        }
        Loader::NeoForge => {
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml".into()
        }
        Loader::Vanilla => unreachable!(),
    };
    let (bytes, cached) = cached_bytes(&paths, &network, &url, 10_000_000)?;
    let mut versions: Vec<LoaderVersion> = match loader {
        Loader::Fabric => serde_json::from_slice::<Value>(&bytes)?
            .as_array()
            .ok_or(CoreError::Integrity)?
            .iter()
            .filter_map(|v| {
                Some(LoaderVersion {
                    id: v["loader"]["version"].as_str()?.into(),
                    stable: v["loader"]["stable"].as_bool().unwrap_or(false),
                })
            })
            .collect(),
        Loader::Forge => serde_json::from_slice::<Value>(&bytes)?[minecraft]
            .as_array()
            .map(|v| {
                v.iter()
                    .rev()
                    .filter_map(|v| {
                        Some(LoaderVersion {
                            id: v.as_str()?.into(),
                            stable: true,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Loader::NeoForge => {
            let text = String::from_utf8(bytes).map_err(|_| CoreError::Integrity)?;
            let prefix = neo_prefix(minecraft);
            regex::Regex::new(r"<version>([^<]+)</version>")
                .unwrap()
                .captures_iter(&text)
                .map(|c| c[1].to_owned())
                .filter(|v| v.starts_with(&prefix))
                .map(|id| LoaderVersion {
                    stable: !id.contains("beta") && !id.contains("alpha"),
                    id,
                })
                .collect()
        }
        Loader::Vanilla => vec![],
    };
    versions.retain(|v| valid_version(&v.id).is_ok());
    if loader == Loader::NeoForge {
        versions.sort_by_key(|v| std::cmp::Reverse(numeric_parts(&v.id)));
    }
    versions.truncate(1000);
    Ok(LoaderCatalog { versions, cached })
}
fn numeric_parts(s: &str) -> Vec<u64> {
    s.split(['.', '-']).filter_map(|v| v.parse().ok()).collect()
}
fn neo_prefix(minecraft: &str) -> String {
    let version = minecraft.strip_prefix("1.").unwrap_or(minecraft);
    if version.contains('.') {
        format!("{version}.")
    } else {
        format!("{version}.0.")
    }
}
pub fn maven_path(name: &str) -> Result<PathBuf, CoreError> {
    let (coordinate, extension) = name.split_once('@').unwrap_or((name, "jar"));
    let pieces: Vec<_> = coordinate.split(':').collect();
    if !(3..=4).contains(&pieces.len()) {
        return Err(CoreError::UnsupportedVersion);
    }
    for p in pieces.iter().copied().chain([extension]) {
        valid_version(p)?;
    }
    let classifier = pieces.get(3).map(|c| format!("-{c}")).unwrap_or_default();
    relative(&format!(
        "{}/{}/{}/{}-{}{}.{}",
        pieces[0].replace('.', "/"),
        pieces[1],
        pieces[2],
        pieces[1],
        pieces[2],
        classifier,
        extension
    ))
}
fn trusted_file(
    paths: &Paths,
    network: &Network,
    url: &str,
    path: &Path,
    expected: Option<&str>,
    manager: &GameManager,
) -> Result<(String, u64), CoreError> {
    let sha = match expected {
        Some(s) => s.to_owned(),
        None => String::from_utf8(cached_bytes(paths, network, &format!("{url}.sha1"), 500)?.0)
            .map_err(|_| CoreError::Integrity)?
            .split_whitespace()
            .next()
            .ok_or(CoreError::Integrity)?
            .to_owned(),
    };
    if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CoreError::Integrity);
    }
    paths.checked(path)?;
    if let Ok(metadata) = path.metadata()
        && verify(path, &Hash::Sha1(sha.clone()), metadata.len())?
    {
        return Ok((sha, metadata.len()));
    }
    if manager.cancelled() {
        return Err(CoreError::Cancelled);
    }
    if let Some(size) = network.artifact_size(url) {
        if size > 150_000_000 {
            return Err(CoreError::Integrity);
        }
        network.download(
            paths,
            &super::network::Download {
                url: url.into(),
                path: path.into(),
                hash: Hash::Sha1(sha.clone()),
                size,
            },
            &|| manager.cancelled(),
            &|n| manager.bytes(n),
        )?;
        return Ok((sha, size));
    }
    let bytes = network.bytes(url, 150_000_000)?;
    if !hex(&sha1::Sha1::digest(&bytes)).eq_ignore_ascii_case(&sha) {
        return Err(CoreError::Integrity);
    }
    manager.bytes(bytes.len() as u64);
    write_atomic(paths, path, &bytes)?;
    Ok((sha, bytes.len() as u64))
}
fn library_key(value: &Value) -> String {
    let name = value["name"].as_str().unwrap_or("");
    let p: Vec<_> = name.split(':').collect();
    format!(
        "{}:{}:{}",
        p.first().unwrap_or(&""),
        p.get(1).unwrap_or(&""),
        p.get(3).unwrap_or(&"")
    )
}
pub fn merge(base: &Value, patch: &Value) -> Result<Value, CoreError> {
    if patch["inheritsFrom"]
        .as_str()
        .is_some_and(|v| Some(v) != base["id"].as_str())
    {
        return Err(CoreError::UnsupportedVersion);
    }
    let mut result = base.clone();
    for field in ["id", "mainClass", "logging"] {
        if let Some(value) = patch.get(field) {
            result[field] = value.clone();
        }
    }
    if let Some(args) = patch.get("minecraftArguments") {
        result["minecraftArguments"] = args.clone();
        result.as_object_mut().unwrap().remove("arguments");
    }
    if let Some(arguments) = patch.get("arguments") {
        if result.get("arguments").is_none() {
            result["arguments"] = json!({"game":base["minecraftArguments"].as_str().unwrap_or("").split_whitespace().collect::<Vec<_>>(),"jvm":["-Djava.library.path=${natives_directory}","-cp","${classpath}"]});
        }
        for side in ["game", "jvm"] {
            if let Some(additions) = arguments[side].as_array() {
                if result["arguments"][side].is_null() {
                    result["arguments"][side] = json!([]);
                }
                result["arguments"][side]
                    .as_array_mut()
                    .ok_or(CoreError::UnsupportedVersion)?
                    .extend(additions.iter().cloned());
            }
        }
    }
    let mut libs = base["libraries"]
        .as_array()
        .ok_or(CoreError::UnsupportedVersion)?
        .clone();
    for lib in patch["libraries"]
        .as_array()
        .ok_or(CoreError::UnsupportedVersion)?
    {
        let key = library_key(lib);
        libs.retain(|v| library_key(v) != key);
        libs.push(lib.clone());
    }
    result["libraries"] = json!(libs);
    Ok(result)
}
fn zip_json(jar: &Path, name: &str) -> Result<Value, CoreError> {
    let mut archive =
        zip::ZipArchive::new(File::open(jar)?).map_err(|_| CoreError::UnsafeArchive)?;
    let entry = archive
        .by_name(name)
        .map_err(|_| CoreError::UnsupportedVersion)?;
    if entry.size() > 10_000_000 {
        return Err(CoreError::Integrity);
    }
    let mut bytes = Vec::new();
    entry.take(10_000_001).read_to_end(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
#[derive(Serialize, Deserialize)]
struct ReceiptFile {
    path: String,
    sha1: String,
    size: u64,
}
fn receipt_files(paths: &Paths, directory: &Path) -> Result<Vec<ReceiptFile>, CoreError> {
    paths.check_tree(directory, 0)?;
    fn walk(root: &Path, dir: &Path, out: &mut Vec<ReceiptFile>) -> Result<(), CoreError> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out)?;
            } else {
                let bytes = fs::read(&path)?;
                out.push(ReceiptFile {
                    path: path
                        .strip_prefix(root)
                        .map_err(|_| CoreError::UnsafePath)?
                        .to_string_lossy()
                        .replace('\\', "/"),
                    sha1: hex(&sha1::Sha1::digest(&bytes)),
                    size: bytes.len() as u64,
                });
            }
        }
        Ok(())
    }
    let mut files = vec![];
    walk(directory, directory, &mut files)?;
    Ok(files)
}
fn receipt_valid(paths: &Paths, root: &Path, receipt: &[ReceiptFile]) -> Result<bool, CoreError> {
    for file in receipt {
        let path = paths.checked(&root.join(relative(&file.path)?))?;
        if !verify(&path, &Hash::Sha1(file.sha1.clone()), file.size)? {
            return Ok(false);
        }
    }
    Ok(!receipt.is_empty())
}
pub fn prepare(
    manager: &GameManager,
    instance: &Instance,
    base: &Value,
    runtime: &JavaRuntime,
    directory: &Path,
) -> Result<(Value, Option<String>), CoreError> {
    if instance.loader == Loader::Vanilla {
        return Ok((base.clone(), None));
    }
    let paths = Paths::new(manager.library.root())?;
    let network = Network::new()?;
    let version = if let Some(v) = &instance.loader_version {
        valid_version(v)?;
        v.clone()
    } else {
        let c = catalog(paths.root(), instance.loader, &instance.minecraft_version)?;
        c.versions
            .iter()
            .find(|v| v.stable)
            .or(c.versions.first())
            .ok_or(CoreError::UnsupportedVersion)?
            .id
            .clone()
    };
    manager.phase(JobPhase::Loader);
    let mut patch = if instance.loader == Loader::Fabric {
        let mut url = reqwest::Url::parse("https://meta.fabricmc.net/v2/versions/loader/").unwrap();
        url.path_segments_mut()
            .unwrap()
            .push(&instance.minecraft_version)
            .push(&version)
            .push("profile")
            .push("json");
        let (bytes, _) = cached_bytes(&paths, &network, url.as_str(), 10_000_000)?;
        serde_json::from_slice::<Value>(&bytes)?
    } else {
        let artifact = if instance.loader == Loader::Forge {
            format!(
                "https://maven.minecraftforge.net/net/minecraftforge/forge/{version}/forge-{version}-installer.jar"
            )
        } else {
            format!(
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar"
            )
        };
        let root = paths
            .root()
            .join("shared/loaders")
            .join(key(instance.loader))
            .join(&version);
        paths.mkdir(&root)?;
        let installer = root.join("installer.jar");
        trusted_file(&paths, &network, &artifact, &installer, None, manager)?;
        let install = zip_json(&installer, "install_profile.json")?;
        if install["minecraft"].as_str() != Some(&instance.minecraft_version) {
            return Err(CoreError::UnsupportedVersion);
        }
        let patch = zip_json(&installer, "version.json").or_else(|_| {
            install
                .get("versionInfo")
                .cloned()
                .ok_or(CoreError::UnsupportedVersion)
        })?;
        let library_root = root.join("libraries");
        let receipt_path = root.join("receipt.json");
        let receipt = read_limited(&paths, &receipt_path, 20_000_000)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<ReceiptFile>>(&b).ok());
        let ready = receipt
            .as_ref()
            .map(|r| receipt_valid(&paths, &library_root, r))
            .transpose()?
            .unwrap_or(false);
        if !ready {
            let version_dir = root
                .join("versions")
                .join(relative(&instance.minecraft_version)?);
            paths.mkdir(&version_dir)?;
            let client = base
                .pointer("/downloads/client")
                .ok_or(CoreError::UnsupportedVersion)?;
            network.download(
                &paths,
                &metadata::artifact(
                    client,
                    version_dir.join(format!("{}.jar", instance.minecraft_version)),
                )?,
                &|| manager.cancelled(),
                &|n| manager.bytes(n),
            )?;
            write_atomic(
                &paths,
                &version_dir.join(format!("{}.json", instance.minecraft_version)),
                &serde_json::to_vec(base)?,
            )?;
            write_atomic(
                &paths,
                &root.join("launcher_profiles.json"),
                br#"{"profiles":{}}"#,
            )?;
            paths.check_tree(&root, 0)?;
            let log = paths.checked(&directory.join("logs").join(format!(
                "install-{}-{}.log",
                key(instance.loader),
                crate::instances::model::now()
            )))?;
            let output = File::create(&log)?;
            let mut child = java::command(Path::new(&runtime.executable))
                .args([
                    "-Djava.awt.headless=true",
                    "-jar",
                    &java_path(&installer),
                    "--installClient",
                    &java_path(&root),
                ])
                .current_dir(&root)
                .stdin(Stdio::null())
                .stdout(output.try_clone()?)
                .stderr(output)
                .spawn()
                .map_err(|_| CoreError::LaunchFailed)?;
            let _group = ProcessGroup::attach(&mut child)?;
            let started = Instant::now();
            loop {
                if manager.cancelled() || started.elapsed() > Duration::from_secs(1200) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(if manager.cancelled() {
                        CoreError::Cancelled
                    } else {
                        CoreError::LaunchFailed
                    });
                }
                if let Some(status) = child.try_wait()? {
                    if !status.success() {
                        return Err(CoreError::LaunchFailed);
                    }
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            // Installer exit alone is insufficient: verify the actual runtime artifacts and outputs.
            for lib in patch["libraries"].as_array().ok_or(CoreError::Integrity)? {
                if let Some(a) = lib.pointer("/downloads/artifact") {
                    let file = library_root.join(relative(string(a, "path")?)?);
                    paths.checked(&file)?;
                    if !verify(
                        &file,
                        &Hash::Sha1(string(a, "sha1")?.into()),
                        a["size"].as_u64().ok_or(CoreError::Integrity)?,
                    )? {
                        return Err(CoreError::Integrity);
                    }
                }
            }
            let receipt = receipt_files(&paths, &library_root)?;
            write_atomic(&paths, &receipt_path, &serde_json::to_vec(&receipt)?)?;
        }
        let receipt: Vec<ReceiptFile> =
            serde_json::from_slice(&read_limited(&paths, &receipt_path, 20_000_000)?)?;
        for file in receipt {
            if manager.cancelled() {
                return Err(CoreError::Cancelled);
            }
            let target = paths
                .root()
                .join("shared/libraries")
                .join(relative(&file.path)?);
            paths.checked(&target)?;
            if !verify(&target, &Hash::Sha1(file.sha1), file.size)? {
                write_atomic(
                    &paths,
                    &target,
                    &read_limited(
                        &paths,
                        &library_root.join(relative(&file.path)?),
                        1_000_000_000,
                    )?,
                )?;
            }
        }
        patch
    };
    for lib in patch["libraries"]
        .as_array_mut()
        .ok_or(CoreError::UnsupportedVersion)?
    {
        if lib.get("downloads").is_none() {
            let path = maven_path(string(lib, "name")?)?;
            let repo = lib["url"]
                .as_str()
                .unwrap_or("https://libraries.minecraft.net/");
            let url = format!(
                "{}{path}",
                repo.trim_end_matches('/').to_owned() + "/",
                path = path.to_string_lossy().replace('\\', "/")
            );
            let destination = paths.root().join("shared/libraries").join(&path);
            let (sha, size) = trusted_file(
                &paths,
                &network,
                &url,
                &destination,
                lib["sha1"].as_str(),
                manager,
            )?;
            lib["downloads"] = json!({"artifact":{"path":path.to_string_lossy().replace('\\',"/"),"url":url,"sha1":sha,"size":size}});
        }
    }
    let result = merge(base, &patch)?;
    Ok((result, Some(version)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neoforge_catalog_does_not_mix_minor_minecraft_versions() {
        assert_eq!(neo_prefix("1.21"), "21.0.");
        assert_eq!(neo_prefix("1.21.1"), "21.1.");
        assert_eq!(neo_prefix("26.1"), "26.1.");
        assert!(!"21.1.252".starts_with(&neo_prefix("1.21")));
    }
    #[test]
    fn rejects_traversal_and_resolves_maven_classifiers() {
        assert!(maven_path("a:b:../bad").is_err());
        assert!(maven_path("a:b:1@../jar").is_err());
        assert_eq!(
            maven_path("net.test:lib:1.2:natives@zip").unwrap(),
            PathBuf::from("net/test/lib/1.2/lib-1.2-natives.zip")
        );
    }
    #[test]
    fn merges_inheritance_and_replaces_conflicting_libraries() {
        let b = json!({"id":"1.21.1","mainClass":"Base","libraries":[{"name":"a:b:1"}],"arguments":{"jvm":["-cp","${classpath}"],"game":["--username","${auth_player_name}"]}});
        let p = json!({"inheritsFrom":"1.21.1","mainClass":"Loader","libraries":[{"name":"a:b:2"}],"arguments":{"game":["--launchTarget","forgeclient"]}});
        let r = merge(&b, &p).unwrap();
        assert_eq!(r["libraries"].as_array().unwrap().len(), 1);
        assert_eq!(r["libraries"][0]["name"], "a:b:2");
        assert_eq!(r["arguments"]["game"].as_array().unwrap().len(), 4);
        let mut wrong = p;
        wrong["inheritsFrom"] = json!("1.20.1");
        assert!(merge(&b, &wrong).is_err());
    }
}
