use super::{
    fs::{read_limited, write_atomic},
    model::*,
    network::Network,
};
use crate::{
    error::CoreError,
    instances::{
        filesystem::Paths,
        model::{game_version, now},
    },
};
use serde::{Deserialize, Serialize};

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
    pub release_time: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub latest: Latest,
    pub versions: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
struct Cached {
    fetched_at: u64,
    manifest: Manifest,
}

pub fn kind(value: &str) -> VersionKind {
    match value {
        "release" => VersionKind::Release,
        "snapshot" => VersionKind::Snapshot,
        "old_beta" => VersionKind::OldBeta,
        "old_alpha" => VersionKind::OldAlpha,
        _ => VersionKind::Other,
    }
}
fn validate(manifest: &Manifest) -> Result<(), CoreError> {
    if manifest.versions.is_empty() || manifest.versions.len() > 10000 {
        return Err(CoreError::Integrity);
    }
    for entry in &manifest.versions {
        game_version(&entry.id)?;
        if entry.sha1.len() != 40
            || !entry.sha1.bytes().all(|c| c.is_ascii_hexdigit())
            || !entry
                .url
                .starts_with("https://piston-meta.mojang.com/v1/packages/")
        {
            return Err(CoreError::Integrity);
        }
    }
    Ok(())
}
pub fn load(
    paths: &Paths,
    network: &Network,
    refresh: bool,
) -> Result<(Manifest, bool), CoreError> {
    let file = paths.root().join("shared/cache/version-catalog.json");
    let cached = read_limited(paths, &file, 8_000_000)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Cached>(&bytes).ok())
        .filter(|cached| validate(&cached.manifest).is_ok());
    if let Some(value) = &cached
        && !refresh
        && now().saturating_sub(value.fetched_at) < 900_000
    {
        return Ok((value.manifest.clone(), false));
    }
    let fetched = network
        .bytes(MANIFEST, 6_000_000)
        .and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).map_err(CoreError::from))
        .and_then(|manifest| {
            validate(&manifest)?;
            Ok(manifest)
        });
    match fetched {
        Ok(manifest) => {
            write_atomic(
                paths,
                &file,
                &serde_json::to_vec(&Cached {
                    fetched_at: now(),
                    manifest: manifest.clone(),
                })?,
            )?;
            Ok((manifest, false))
        }
        Err(error) => cached.map(|value| (value.manifest, true)).ok_or(error),
    }
}
/// Known versions can be prepared offline from a previously validated catalog.
/// Catalog browsing/explicit refresh still checks for new releases and metadata updates.
pub fn for_version(paths: &Paths, network: &Network, id: &str) -> Result<Manifest, CoreError> {
    let file = paths.root().join("shared/cache/version-catalog.json");
    if let Ok(bytes) = read_limited(paths, &file, 8_000_000)
        && let Ok(cached) = serde_json::from_slice::<Cached>(&bytes)
        && validate(&cached.manifest).is_ok()
        && cached.manifest.versions.iter().any(|v| v.id == id)
    {
        return Ok(cached.manifest);
    }
    load(paths, network, false).map(|(manifest, _)| manifest)
}
pub fn catalog(
    paths: &Paths,
    network: &Network,
    refresh: bool,
) -> Result<VersionCatalog, CoreError> {
    let (manifest, cached) = load(paths, network, refresh)?;
    Ok(VersionCatalog {
        versions: manifest
            .versions
            .into_iter()
            .map(|entry| GameVersion {
                id: entry.id,
                kind: kind(&entry.kind),
                released_at: entry.release_time,
            })
            .collect(),
        latest_release: manifest.latest.release,
        latest_snapshot: manifest.latest.snapshot,
        cached,
    })
}
