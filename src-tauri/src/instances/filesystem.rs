use super::model::{INSTANCE_SCHEMA, valid_id};
use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

pub const INSTANCE_DIRECTORIES: &[&str] = &[
    "mods",
    "mods_disabled",
    "config",
    "saves",
    "resourcepacks",
    "shaderpacks",
    "screenshots",
    "logs",
];

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Marker {
    schema_version: u32,
    id: String,
}

#[derive(Debug)]
pub struct Paths {
    root: PathBuf,
}

fn ordinary(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !metadata.file_type().is_symlink() && (metadata.is_file() || metadata.is_dir())
}

pub(crate) fn no_links(path: &Path) -> Result<(), CoreError> {
    let mut cursor = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(CoreError::UnsafePath);
        }
        cursor.push(component);
        // A Windows drive/verbatim prefix is not an independently queryable path.
        if matches!(component, Component::Prefix(_)) || !cursor.is_absolute() {
            continue;
        }
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) if !ordinary(&metadata) => return Err(CoreError::UnsafePath),
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

impl Paths {
    pub fn new(root: &Path) -> Result<Self, CoreError> {
        if !root.is_absolute() {
            return Err(CoreError::UnsafePath);
        }
        no_links(root)?;
        fs::create_dir_all(root)?;
        let paths = Self {
            root: root.canonicalize()?,
        };
        for directory in [
            "launcher",
            "launcher/staging",
            "launcher/trash",
            "launcher/instance-locks",
            "instances",
            "backups",
            "shared/assets",
            "shared/libraries",
            "shared/runtimes",
            "shared/cache",
        ] {
            paths.mkdir(&paths.root.join(directory))?;
        }
        Ok(paths)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn checked(&self, path: &Path) -> Result<PathBuf, CoreError> {
        if !path.starts_with(&self.root)
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(CoreError::UnsafePath);
        }
        no_links(path)?;
        if path.exists() && !path.canonicalize()?.starts_with(&self.root) {
            return Err(CoreError::UnsafePath);
        }
        Ok(path.to_path_buf())
    }

    pub fn location(&self, parent: &str, id: &str) -> Result<PathBuf, CoreError> {
        valid_id(id)?;
        self.checked(&self.root.join(parent).join(id))
    }

    pub fn mkdir(&self, path: &Path) -> Result<(), CoreError> {
        self.checked(path)?;
        fs::create_dir_all(path)?;
        Ok(())
    }

    pub fn lock(&self) -> Result<File, CoreError> {
        let path = self.checked(&self.root.join("launcher/instances.lock"))?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock
                || error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
            {
                CoreError::LibraryBusy
            } else {
                CoreError::Io(error)
            }
        })?;
        Ok(file)
    }

    pub fn instance_lock(&self, id: &str) -> Result<File, CoreError> {
        valid_id(id)?;
        self.named_lock(
            &self
                .root
                .join("launcher/instance-locks")
                .join(format!("{id}.lock")),
        )
    }

    pub fn named_lock(&self, path: &Path) -> Result<File, CoreError> {
        self.named_lock_mode(path, false)
    }

    pub(crate) fn instance_read_lock(&self, id: &str) -> Result<File, CoreError> {
        valid_id(id)?;
        self.named_lock_mode(
            &self
                .root
                .join("launcher/instance-locks")
                .join(format!("{id}.lock")),
            true,
        )
    }

    fn named_lock_mode(&self, path: &Path, shared: bool) -> Result<File, CoreError> {
        let path = self.checked(path)?;
        self.mkdir(path.parent().ok_or(CoreError::UnsafePath)?)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        let result = if shared {
            fs2::FileExt::try_lock_shared(&file)
        } else {
            fs2::FileExt::try_lock_exclusive(&file)
        };
        result.map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock
                || error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
            {
                CoreError::InstanceBusy
            } else {
                CoreError::Io(error)
            }
        })?;
        Ok(file)
    }

    pub fn write_json(&self, path: &Path, value: &impl Serialize) -> Result<(), CoreError> {
        self.checked(path)?;
        let bytes = serde_json::to_vec_pretty(value)?;
        if path.exists() {
            if fs::read(path)? == bytes {
                return Ok(());
            }
            return Err(CoreError::UnsafePath);
        }
        let mut temporary =
            tempfile::NamedTempFile::new_in(path.parent().ok_or(CoreError::UnsafePath)?)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist_noclobber(path)
            .map_err(|error| CoreError::Io(error.error))?;
        Ok(())
    }

    pub fn marker(&self, path: &Path, id: &str) -> Result<(), CoreError> {
        self.write_json(
            &path.join("instance.json"),
            &Marker {
                schema_version: INSTANCE_SCHEMA,
                id: id.into(),
            },
        )
    }

    pub fn verify_marker(&self, path: &Path, id: &str) -> Result<(), CoreError> {
        let marker_path = self.checked(&path.join("instance.json"))?;
        if fs::metadata(&marker_path)?.len() > 4096 {
            return Err(CoreError::UnsafePath);
        }
        let marker: Marker = serde_json::from_slice(&fs::read(marker_path)?)?;
        if marker.schema_version > INSTANCE_SCHEMA {
            return Err(CoreError::SchemaTooNew);
        }
        if marker.schema_version != INSTANCE_SCHEMA || marker.id != id {
            return Err(CoreError::UnsafePath);
        }
        Ok(())
    }

    pub fn prepare(&self, id: &str) -> Result<PathBuf, CoreError> {
        let stage = self.location("launcher/staging", id)?;
        fs::create_dir(&stage)?;
        self.marker(&stage, id)?;
        Ok(stage)
    }

    pub fn check_tree(&self, path: &Path, depth: usize) -> Result<(), CoreError> {
        if depth > 128 {
            return Err(CoreError::UnsafePath);
        }
        self.checked(path)?;
        if fs::symlink_metadata(path)?.is_dir() {
            for entry in fs::read_dir(path)? {
                self.check_tree(&entry?.path(), depth + 1)?;
            }
        }
        Ok(())
    }

    pub fn copy_tree(&self, source: &Path, target: &Path, depth: usize) -> Result<(), CoreError> {
        if depth > 128 {
            return Err(CoreError::UnsafePath);
        }
        self.checked(source)?;
        self.checked(target)?;
        let before = fs::symlink_metadata(source)?;
        if before.is_dir() {
            if !target.exists() {
                fs::create_dir(target)?;
            }
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                // Identity of the duplicate is newly generated, never copied from its source.
                if depth == 0 && entry.file_name() == "instance.json" {
                    continue;
                }
                self.copy_tree(&entry.path(), &target.join(entry.file_name()), depth + 1)?;
            }
        } else {
            let mut source_file = File::open(source)?;
            let mut target_file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(target)?;
            std::io::copy(&mut source_file, &mut target_file)?;
            target_file.sync_all()?;
        }
        let after = fs::symlink_metadata(source)?;
        if !ordinary(&after)
            || before.modified().ok() != after.modified().ok()
            || (before.is_file() && before.len() != after.len())
        {
            return Err(CoreError::SourceChanged);
        }
        Ok(())
    }

    pub fn rename(&self, source: &Path, target: &Path) -> Result<(), CoreError> {
        self.checked(source)?;
        self.checked(target)?;
        if target.exists() {
            return Err(CoreError::UnsafePath);
        }
        fs::rename(source, target)?;
        Ok(())
    }

    pub fn remove(&self, path: &Path) -> Result<(), CoreError> {
        self.checked(path)?;
        if path == self.root || path.parent() == Some(self.root.as_path()) {
            return Err(CoreError::UnsafePath);
        }
        if !path.exists() {
            return Ok(());
        }
        self.check_tree(path, 0)?;
        if fs::symlink_metadata(path)?.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}
