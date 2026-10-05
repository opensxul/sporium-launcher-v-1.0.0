use crate::{error::CoreError, instances::filesystem::Paths};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub fn relative(value: &str) -> Result<PathBuf, CoreError> {
    if value.is_empty() || value.contains(['\\', ':', '\0']) {
        return Err(CoreError::UnsafePath);
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(CoreError::UnsafePath);
    }
    for part in path.components() {
        let Component::Normal(name) = part else {
            return Err(CoreError::UnsafePath);
        };
        let name = name.to_str().ok_or(CoreError::UnsafePath)?;
        let base = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if name.ends_with(['.', ' '])
            || name.chars().any(char::is_control)
            || name.contains(['<', '>', '"', '|', '?', '*'])
            || ["CON", "PRN", "AUX", "NUL"].contains(&base.as_str())
            || (base.len() == 4
                && (base.starts_with("COM") || base.starts_with("LPT"))
                && base.as_bytes()[3].is_ascii_digit())
        {
            return Err(CoreError::UnsafePath);
        }
    }
    Ok(path.to_path_buf())
}

pub fn write_atomic(paths: &Paths, path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    paths.checked(path)?;
    let parent = path.parent().ok_or(CoreError::UnsafePath)?;
    paths.mkdir(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    paths.checked(path)?;
    temporary
        .persist(path)
        .map_err(|error| CoreError::Io(error.error))?;
    Ok(())
}

pub fn read_limited(paths: &Paths, path: &Path, limit: u64) -> Result<Vec<u8>, CoreError> {
    paths.checked(path)?;
    let file = File::open(path)?;
    if file.metadata()?.len() > limit {
        return Err(CoreError::Integrity);
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(CoreError::Integrity);
    }
    Ok(bytes)
}

pub fn extract(
    paths: &Paths,
    source: &Path,
    destination: &Path,
    excludes: &[String],
    cancelled: &impl Fn() -> bool,
) -> Result<(), CoreError> {
    paths.checked(source)?;
    paths.mkdir(destination)?;
    let mut zip =
        zip::ZipArchive::new(File::open(source)?).map_err(|_| CoreError::UnsafeArchive)?;
    if zip.len() > 25000 {
        return Err(CoreError::UnsafeArchive);
    }
    let mut expanded = 0u64;
    for index in 0..zip.len() {
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        let mut entry = zip.by_index(index).map_err(|_| CoreError::UnsafeArchive)?;
        let name = entry.name().trim_end_matches('/');
        let part = relative(name).map_err(|_| CoreError::UnsafeArchive)?;
        if entry.enclosed_name().is_none()
            || entry.unix_mode().is_some_and(|mode| {
                mode & 0o170000 != 0 && mode & 0o170000 != 0o100000 && mode & 0o170000 != 0o040000
            })
        {
            return Err(CoreError::UnsafeArchive);
        }
        if excludes
            .iter()
            .any(|prefix| entry.name().starts_with(prefix))
        {
            continue;
        }
        expanded = expanded
            .checked_add(entry.size())
            .ok_or(CoreError::UnsafeArchive)?;
        if expanded > 2_000_000_000 || entry.size() > 500_000_000 {
            return Err(CoreError::UnsafeArchive);
        }
        let target = paths.checked(&destination.join(part))?;
        if entry.is_dir() {
            paths.mkdir(&target)?;
            continue;
        }
        paths.mkdir(target.parent().ok_or(CoreError::UnsafeArchive)?)?;
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)?;
        let expected = entry.size();
        let copied = std::io::copy(&mut entry.by_ref().take(expected + 1), &mut output)?;
        if copied != expected {
            return Err(CoreError::Integrity);
        }
        output.sync_all()?;
    }
    Ok(())
}

pub fn java_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{unc}")
    } else {
        value.strip_prefix("\\\\?\\").unwrap_or(&value).to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn extraction_rejects_traversal_and_preserves_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(&dir.path().join("data")).unwrap();
        let source = paths.root().join("archive.zip");
        let target = paths.root().join("extract");
        archive(&source, &[("../outside", b"forbidden")]);
        assert!(matches!(
            extract(&paths, &source, &target, &[], &|| false),
            Err(CoreError::UnsafeArchive)
        ));
        assert!(!paths.root().join("outside").exists());
        archive(&source, &[("native.dll", b"verified")]);
        extract(&paths, &source, &target, &[], &|| false).unwrap();
        archive(&source, &[("native.dll", b"overwrite")]);
        assert!(extract(&paths, &source, &target, &[], &|| false).is_err());
        assert_eq!(fs::read(target.join("native.dll")).unwrap(), b"verified");
    }

    #[test]
    fn extraction_obeys_excludes_and_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path()).unwrap();
        let source = paths.root().join("archive.zip");
        archive(
            &source,
            &[("META-INF/manifest", b"skip"), ("bin/native.dll", b"dll")],
        );
        let target = paths.root().join("extract");
        extract(&paths, &source, &target, &["META-INF/".into()], &|| false).unwrap();
        assert!(!target.join("META-INF").exists());
        assert_eq!(fs::read(target.join("bin/native.dll")).unwrap(), b"dll");
        let cancelled = paths.root().join("cancelled");
        assert!(matches!(
            extract(&paths, &source, &cancelled, &[], &|| true),
            Err(CoreError::Cancelled)
        ));
        assert_eq!(fs::read_dir(cancelled).unwrap().count(), 0);
    }
}
