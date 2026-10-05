//! Cleanup owns only disposable caches. Game assets, clients, libraries, Java and instances stay intact.
use crate::{error::CoreError, instances::filesystem::Paths};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};
use ts_rs::TS;

const DISPOSABLE: &[&str] = &[
    "shared/cache/modrinth",
    "shared/cache/java",
    "shared/cache/parts",
    "shared/cache/skins",
    "shared/cache/loaders",
];
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    #[ts(type = "number")]
    pub disposable_bytes: u64,
    #[ts(type = "number")]
    pub partial_bytes: u64,
    pub files: u32,
}
struct Entry {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
}
fn entries(paths: &Paths) -> Result<Vec<Entry>, CoreError> {
    let mut output = vec![];
    fn walk(paths: &Paths, dir: &Path, out: &mut Vec<Entry>) -> Result<(), CoreError> {
        paths.checked(dir)?;
        if !dir.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = paths.checked(&entry.path())?;
            let meta = entry.metadata()?;
            if meta.is_dir() {
                walk(paths, &path, out)?;
            } else if path.extension().is_none_or(|ext| ext != "lock") {
                out.push(Entry {
                    path,
                    size: meta.len(),
                    modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                });
            }
        }
        Ok(())
    }
    for dir in DISPOSABLE {
        walk(paths, &paths.root().join(dir), &mut output)?;
    }
    Ok(output)
}
pub fn stats(root: &Path) -> Result<CacheStats, CoreError> {
    let paths = Paths::new(root)?;
    let entries = entries(&paths)?;
    Ok(CacheStats {
        disposable_bytes: entries.iter().map(|e| e.size).sum(),
        partial_bytes: entries
            .iter()
            .filter(|e| e.path.extension().is_some_and(|s| s == "part"))
            .map(|e| e.size)
            .sum(),
        files: entries.len() as u32,
    })
}
pub fn trim(root: &Path, limit: u64) -> Result<CacheStats, CoreError> {
    let paths = Paths::new(root)?;
    let _lease = paths.named_lock(&paths.root().join("shared/cache/downloads.lock"))?;
    let mut entries = entries(&paths)?;
    let mut total: u64 = entries.iter().map(|e| e.size).sum();
    entries.sort_by_key(|e| e.modified);
    for entry in entries {
        if total <= limit {
            break;
        }
        paths.remove(&entry.path)?;
        total = total.saturating_sub(entry.size);
    }
    stats(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_preserves_clients_libraries_worlds_and_holds_the_download_lock() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::new(root.path()).unwrap();
        for relative in [
            "shared/cache/parts/x.part",
            "shared/cache/skins/nick.json",
            "shared/cache/clients/client.jar",
            "shared/libraries/a.jar",
            "instances/world/saves/world.dat",
        ] {
            let path = paths.root().join(relative);
            paths.mkdir(path.parent().unwrap()).unwrap();
            fs::write(path, b"keep or clear").unwrap();
        }
        assert_eq!(stats(root.path()).unwrap().files, 2);
        let lock = paths
            .named_lock(&paths.root().join("shared/cache/downloads.lock"))
            .unwrap();
        assert!(trim(root.path(), 0).is_err());
        drop(lock);
        assert_eq!(trim(root.path(), 0).unwrap().files, 0);
        for relative in [
            "shared/cache/clients/client.jar",
            "shared/libraries/a.jar",
            "instances/world/saves/world.dat",
        ] {
            assert_eq!(
                fs::read(paths.root().join(relative)).unwrap(),
                b"keep or clear"
            );
        }
    }
}
