use super::{
    Library,
    filesystem::no_links,
    model::{FolderTarget, OpenFolder},
};
use crate::error::CoreError;
use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSummary {
    pub enabled_mods: u32,
    pub disabled_mods: u32,
    #[ts(type = "number | null")]
    pub directory_bytes: Option<u64>,
}

impl Library {
    /// Metadata only, with a bounded traversal outside the library database lock.
    /// Shared Java/game assets are intentionally excluded from the instance directory size.
    pub fn summary(&self, id: &str) -> Result<InstanceSummary, CoreError> {
        let root = self.folder(OpenFolder {
            target: FolderTarget::Instance,
            id: Some(id.into()),
        })?;
        let count = |name: &str| -> Result<u32, CoreError> {
            let directory = root.join(name);
            no_links(&directory)?;
            let mut total = 0;
            for (index, entry) in std::fs::read_dir(directory)?.enumerate() {
                if index >= 50_000 {
                    return Err(CoreError::InvalidInput);
                }
                let entry = entry?;
                if entry.file_type()?.is_file()
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
                {
                    total += 1;
                }
            }
            Ok(total)
        };
        let mut result = InstanceSummary {
            enabled_mods: count("mods")?,
            disabled_mods: count("mods_disabled")?,
            directory_bytes: None,
        };
        let mut pending = vec![(root, 0)];
        let mut entries = 0;
        let mut bytes = 0u64;
        while let Some((directory, depth)) = pending.pop() {
            if depth > 64 {
                return Ok(result);
            }
            no_links(&directory)?;
            for entry in std::fs::read_dir(directory)? {
                let path = entry?.path();
                entries += 1;
                if entries > 50_000 {
                    return Ok(result);
                }
                no_links(&path)?;
                let metadata = std::fs::symlink_metadata(&path)?;
                if metadata.is_dir() {
                    pending.push((path, depth + 1));
                } else if metadata.is_file() {
                    bytes = bytes
                        .checked_add(metadata.len())
                        .ok_or(CoreError::InvalidInput)?;
                }
            }
        }
        result.directory_bytes = Some(bytes);
        Ok(result)
    }
}
