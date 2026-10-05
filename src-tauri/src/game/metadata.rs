use super::{
    catalog::Entry,
    fs::{read_limited, relative, write_atomic},
    network::{Download, Hash, Network},
    rules::{self, Environment},
};
use crate::{error::CoreError, instances::filesystem::Paths};
use serde_json::Value;
use sha1::Digest;
use std::path::PathBuf;

pub fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, CoreError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(CoreError::UnsupportedVersion)
}
pub fn artifact(value: &Value, path: PathBuf) -> Result<Download, CoreError> {
    let sha1 = string(value, "sha1")?;
    if sha1.len() != 40 || !sha1.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(CoreError::Integrity);
    }
    Ok(Download {
        url: string(value, "url")?.into(),
        path,
        hash: Hash::Sha1(sha1.into()),
        size: value
            .get("size")
            .and_then(Value::as_u64)
            .filter(|size| *size <= 1_000_000_000)
            .ok_or(CoreError::Integrity)?,
    })
}
pub fn version(paths: &Paths, network: &Network, entry: &Entry) -> Result<Value, CoreError> {
    let path = paths
        .root()
        .join("shared/cache/versions")
        .join(format!("{}.json", entry.sha1));
    let valid = |bytes: &[u8]| super::network::hex(&sha1::Sha1::digest(bytes)) == entry.sha1;
    let bytes = match read_limited(paths, &path, 10_000_000) {
        Ok(bytes) if valid(&bytes) => bytes,
        _ => {
            let bytes = network.bytes(&entry.url, 10_000_000)?;
            if !valid(&bytes) {
                return Err(CoreError::Integrity);
            }
            write_atomic(paths, &path, &bytes)?;
            bytes
        }
    };
    let value: Value = serde_json::from_slice(&bytes)?;
    if string(&value, "id")? != entry.id || value.get("inheritsFrom").is_some() {
        return Err(CoreError::UnsupportedVersion);
    }
    Ok(value)
}

pub struct Libraries {
    pub downloads: Vec<Download>,
    pub classpath: Vec<PathBuf>,
    pub natives: Vec<(PathBuf, Vec<String>)>,
}
pub fn libraries(
    paths: &Paths,
    metadata: &Value,
    env: &Environment,
) -> Result<Libraries, CoreError> {
    let mut result = Libraries {
        downloads: vec![],
        classpath: vec![],
        natives: vec![],
    };
    for library in metadata
        .get("libraries")
        .and_then(Value::as_array)
        .ok_or(CoreError::UnsupportedVersion)?
    {
        if !rules::allowed(library.get("rules"), env)? {
            continue;
        }
        let downloads = library
            .get("downloads")
            .ok_or(CoreError::UnsupportedVersion)?;
        if let Some(value) = downloads.get("artifact") {
            let path = paths.checked(
                &paths
                    .root()
                    .join("shared/libraries")
                    .join(relative(string(value, "path")?)?),
            )?;
            if !result.classpath.contains(&path) {
                result.classpath.push(path.clone());
            }
            result.downloads.push(artifact(value, path)?);
        }
        if let Some(classifier) = library
            .get("natives")
            .and_then(|value| value.get(&env.os))
            .and_then(Value::as_str)
        {
            let classifier = classifier.replace("${arch}", "64");
            let value = downloads
                .get("classifiers")
                .and_then(|value| value.get(&classifier))
                .ok_or(CoreError::UnsupportedVersion)?;
            let path = paths.checked(
                &paths
                    .root()
                    .join("shared/libraries")
                    .join(relative(string(value, "path")?)?),
            )?;
            let exclude = library
                .pointer("/extract/exclude")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_else(|| vec!["META-INF/".into()]);
            // Historical official manifests repeat the same jinput native library.
            if !result.natives.iter().any(|(source, _)| source == &path) {
                result.natives.push((path.clone(), exclude));
            }
            result.downloads.push(artifact(value, path)?);
        }
    }
    let client = metadata
        .pointer("/downloads/client")
        .ok_or(CoreError::UnsupportedVersion)?;
    let client_path = paths
        .root()
        .join("shared/cache/clients")
        .join(format!("{}.jar", string(client, "sha1")?));
    result.classpath.push(client_path.clone());
    result.downloads.push(artifact(client, client_path)?);
    Ok(result)
}

pub struct Assets {
    pub downloads: Vec<Download>,
    pub virtual_files: Vec<(PathBuf, PathBuf)>,
    pub game_assets: PathBuf,
    pub id: String,
}
pub fn assets(
    paths: &Paths,
    network: &Network,
    metadata: &Value,
    game_dir: &std::path::Path,
    cancelled: &impl Fn() -> bool,
    progress: &impl Fn(u64),
) -> Result<Assets, CoreError> {
    let descriptor = metadata
        .get("assetIndex")
        .ok_or(CoreError::UnsupportedVersion)?;
    let id = string(descriptor, "id")?.to_owned();
    relative(&id)?;
    let path = paths
        .root()
        .join("shared/assets/indexes")
        .join(format!("{id}.json"));
    network.download(
        paths,
        &artifact(descriptor, path.clone())?,
        cancelled,
        progress,
    )?;
    let value: Value = serde_json::from_slice(&read_limited(paths, &path, 16_000_000)?)?;
    let virtual_root = paths.root().join("shared/assets/virtual").join(&id);
    let mapped = value
        .get("map_to_resources")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let game_assets = if mapped {
        game_dir.join("resources")
    } else {
        virtual_root.clone()
    };
    let mut result = Assets {
        downloads: vec![],
        virtual_files: vec![],
        game_assets,
        id,
    };
    let objects = value
        .get("objects")
        .and_then(Value::as_object)
        .ok_or(CoreError::UnsupportedVersion)?;
    if objects.len() > 100_000 {
        return Err(CoreError::Integrity);
    }
    let mut seen = std::collections::HashSet::new();
    for (name, entry) in objects {
        let name = relative(name)?;
        let hash = string(entry, "hash")?;
        if hash.len() != 40 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(CoreError::Integrity);
        }
        let object = paths
            .root()
            .join("shared/assets/objects")
            .join(&hash[..2])
            .join(hash);
        if seen.insert(hash.to_owned()) {
            result.downloads.push(Download {
                url: format!(
                    "https://resources.download.minecraft.net/{}/{}",
                    &hash[..2],
                    hash
                ),
                path: object.clone(),
                hash: Hash::Sha1(hash.into()),
                size: entry
                    .get("size")
                    .and_then(Value::as_u64)
                    .filter(|size| *size < 1_000_000_000)
                    .ok_or(CoreError::Integrity)?,
            });
        }
        if value
            .get("virtual")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            result
                .virtual_files
                .push((object.clone(), virtual_root.join(&name)));
        }
        if value
            .get("map_to_resources")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            result
                .virtual_files
                .push((object, game_dir.join("resources").join(&name)));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn repeated_legacy_native_descriptor_is_extracted_once() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path()).unwrap();
        let native = json!({"natives":{"windows":"natives-windows"}, "downloads":{"classifiers":{"natives-windows":{
            "path":"jinput/native.jar", "sha1":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "size":3,
            "url":"https://libraries.minecraft.net/jinput/native.jar"
        }}}});
        let metadata = json!({"libraries":[native.clone(), native], "downloads":{"client":{
            "sha1":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "size":3, "url":"https://piston-data.mojang.com/client.jar"
        }}});
        let result = libraries(&paths, &metadata, &Environment::windows(false)).unwrap();
        assert_eq!(result.natives.len(), 1);
    }
}
