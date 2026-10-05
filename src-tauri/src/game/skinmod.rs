//! Optional, unmodified CustomSkinLoader release, shared across local nicknames.
use super::{
    GameManager,
    fs::{read_limited, write_atomic},
    network::{Download, Hash, Network, verify},
};
use crate::{
    error::CoreError,
    instances::{
        filesystem::Paths,
        model::{Instance, Loader},
    },
};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct Release {
    minecraft: Vec<String>,
    url: String,
    sha1: String,
    size: u64,
}
const OWNED_NAME: &str = "Sporium-CustomSkinLoader-15.0.1.jar";

pub(crate) fn owned(path: &Path) -> Result<bool, CoreError> {
    if !path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(OWNED_NAME))
    {
        return Ok(false);
    }
    let release: Release = serde_json::from_str(include_str!("../../skinmod.json"))?;
    verify(path, &Hash::Sha1(release.sha1), release.size)
}

pub fn prepare(
    manager: &GameManager,
    instance: &Instance,
    directory: &Path,
    enabled: bool,
) -> Result<(), CoreError> {
    let paths = Paths::new(manager.library.root())?;
    let release: Release = serde_json::from_str(include_str!("../../skinmod.json"))?;
    let destination = paths.checked(&directory.join("mods").join(OWNED_NAME))?;
    let owned = verify(
        &destination,
        &Hash::Sha1(release.sha1.clone()),
        release.size,
    )?;
    if !enabled
        || instance.loader == Loader::Vanilla
        || !release.minecraft.contains(&instance.minecraft_version)
    {
        // Only remove the exact byte-for-byte file managed by Sporium, never user changes.
        if owned {
            paths.remove(&destination)?;
        }
        return Ok(());
    }
    paths.mkdir(&directory.join("mods"))?;
    paths.check_tree(&directory.join("mods"), 0)?;
    // Respect a user's own CustomSkinLoader installation and configuration.
    for entry in std::fs::read_dir(directory.join("mods"))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name != OWNED_NAME
            && name.to_lowercase().contains("customskinloader")
            && name.ends_with(".jar")
        {
            if owned {
                paths.remove(&destination)?;
            }
            return Ok(());
        }
    }
    if destination.exists() && !owned {
        return Ok(());
    }
    let source = paths.root().join("shared/skins").join(OWNED_NAME);
    Network::with_timeout(std::time::Duration::from_secs(20))?.download(
        &paths,
        &Download {
            url: release.url,
            path: source.clone(),
            hash: Hash::Sha1(release.sha1),
            size: release.size,
        },
        &|| manager.cancelled(),
        &|n| manager.bytes(n),
    )?;
    let config = paths.checked(&directory.join("CustomSkinLoader/CustomSkinLoader.json"))?;
    if !config.exists() {
        write_atomic(&paths, &config, br#"{"version":"15.0.1","buildNumber":0,"loadlist":[{"name":"Mojang","type":"MojangAPI"}],"enableLocalProfileCache":true}"#)?;
    }
    if !owned {
        write_atomic(
            &paths,
            &destination,
            &read_limited(&paths, &source, 2_000_000)?,
        )?;
    }
    Ok(())
}
