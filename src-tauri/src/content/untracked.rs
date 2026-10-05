use super::{install, local, model::UntrackedContent};
use crate::{
    error::CoreError,
    instances::{filesystem::Paths, model::Instance},
};
use std::{collections::HashSet, path::Path};

/// A read-only inventory, deliberately separate from the installation receipt.
pub(super) fn scan(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
) -> Result<Vec<UntrackedContent>, CoreError> {
    let tracked: HashSet<_> = install::records(paths, directory)?
        .into_iter()
        .map(|record| format!("{}/{}", record.directory, record.file.filename).to_lowercase())
        .collect();
    let mut result = vec![];
    let mut count = 0;
    for (folder, kind) in [
        ("mods", "mod"),
        ("mods_disabled", "mod"),
        ("resourcepacks", "resourcepack"),
        ("shaderpacks", "shader"),
    ] {
        let root = paths.checked(&directory.join(folder))?;
        if !root.exists() {
            continue;
        }
        for entry in std::fs::read_dir(root)? {
            count += 1;
            if count > 4096 {
                return Err(CoreError::Integrity);
            }
            let entry = entry?;
            let filename = entry.file_name().to_string_lossy().into_owned();
            if tracked.contains(&format!("{folder}/{filename}").to_lowercase()) {
                continue;
            }
            let path = paths.checked(&entry.path())?;
            let is_file = path.is_file();
            let supported = if kind == "mod" {
                is_file
                    && path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("jar"))
            } else {
                path.is_dir()
                    || (is_file
                        && path
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("zip")))
            };
            if !supported {
                continue;
            }
            let mut row = UntrackedContent {
                manageable: is_file,
                directory: folder.into(),
                filename: filename.clone(),
                kind: kind.into(),
                title: filename,
                version: String::new(),
                metadata: None,
                status: "unknown".into(),
                disabled: folder == "mods_disabled",
            };
            if kind == "mod" {
                match local::inspect(&path, instance) {
                    Ok((title, version, metadata)) => {
                        if !title.is_empty() {
                            row.title = title;
                        }
                        row.version = version;
                        row.metadata = Some(metadata);
                    }
                    Err(CoreError::ContentIncompatible) => row.status = "incompatible".into(),
                    Err(_) => row.status = "unreadable".into(),
                }
            }
            if is_file && crate::game::skinmod::owned(&path)? {
                row.status = "launcher".into();
                row.manageable = false;
            } else if row.status != "unknown" {
                row.manageable = false;
            }
            result.push(row);
        }
    }
    result.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then(a.filename.cmp(&b.filename))
    });
    Ok(result)
}
