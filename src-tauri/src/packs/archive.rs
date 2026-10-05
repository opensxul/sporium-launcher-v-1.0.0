use super::model::*;
use crate::{
    error::CoreError,
    game::{
        fs::relative,
        network::{Hash, hex, verify},
    },
    instances::{
        filesystem::{Paths, no_links},
        model::{CreateInstance, Loader},
    },
};
use sha1::Digest;
use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::{Read, Write},
    path::Path,
};

pub const MAX_BYTES: u64 = 2_000_000_000;
pub const MAX_FILE: u64 = 500_000_000;
pub const MAX_FILES: usize = 10000;

pub fn allowed_path(value: &str) -> Result<(), CoreError> {
    let part = relative(value)?;
    // Path::components normalizes `a/./b` and repeated separators; do not accept aliases.
    if part.to_string_lossy().replace('\\', "/") != value
        || value
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(CoreError::UnsafePath);
    }
    let top = value.split('/').next().unwrap_or_default().to_lowercase();
    if !matches!(
        top.as_str(),
        "mods"
            | "mods_disabled"
            | "config"
            | "resourcepacks"
            | "shaderpacks"
            | "saves"
            | "defaultconfigs"
            | "kubejs"
            | "scripts"
            | "datapacks"
            | "options.txt"
            | "optionsof.txt"
            | "optionsshaders.txt"
            | "servers.dat"
    ) {
        return Err(CoreError::ContentUnsupported);
    }
    if matches!(
        top.as_str(),
        "options.txt" | "optionsof.txt" | "optionsshaders.txt" | "servers.dat"
    ) && value.contains('/')
    {
        return Err(CoreError::UnsafePath);
    }
    Ok(())
}
pub fn hash_valid(hash: &str, len: usize) -> bool {
    hash.len() == len && hash.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn pack_url(value: &str) -> bool {
    reqwest::Url::parse(value).ok().is_some_and(|url| {
        !value.chars().any(char::is_whitespace)
            && crate::game::network::approved(&url)
            && url.host_str() == Some("cdn.modrinth.com")
    })
}
pub fn metadata(manifest: &Manifest) -> Result<(CreateInstance, Option<String>), CoreError> {
    if manifest.format_version != 1 {
        return Err(CoreError::SchemaTooNew);
    }
    if manifest.game != "minecraft"
        || manifest.files.len() > MAX_FILES
        || manifest.version_id.is_empty()
        || manifest.version_id.len() > 180
        || manifest.summary.as_ref().is_some_and(|s| s.len() > 20000)
    {
        return Err(CoreError::ContentUnsupported);
    }
    let mut loader = Loader::Vanilla;
    let mut version = None;
    for (key, value) in &manifest.dependencies {
        let candidate = match key.as_str() {
            "minecraft" => continue,
            "fabric-loader" => Loader::Fabric,
            "forge" => Loader::Forge,
            "neoforge" => Loader::NeoForge,
            _ => return Err(CoreError::ContentUnsupported),
        };
        if version.is_some() {
            return Err(CoreError::DependencyConflict);
        }
        crate::game::loaders::valid_version(value)?;
        loader = candidate;
        version = Some(value.clone());
    }
    let request = CreateInstance {
        name: manifest.name.clone(),
        minecraft_version: manifest
            .dependencies
            .get("minecraft")
            .cloned()
            .ok_or(CoreError::InvalidInput)?,
        loader,
        collection_id: None,
    };
    crate::instances::Library::draft(&request)?;
    let mut seen = HashSet::new();
    let mut bytes = 0u64;
    for file in &manifest.files {
        allowed_path(&file.path)?;
        if !seen.insert(file.path.to_lowercase())
            || !file.hashes.get("sha1").is_some_and(|h| hash_valid(h, 40))
            || !file
                .hashes
                .get("sha512")
                .is_some_and(|h| hash_valid(h, 128))
            || file.file_size > MAX_FILE
            || file.downloads.is_empty()
            || file.downloads.len() > 8
            || file.downloads.iter().any(|u| !pack_url(u))
        {
            return Err(CoreError::Integrity);
        }
        bytes = bytes
            .checked_add(file.file_size)
            .ok_or(CoreError::Integrity)?;
        if bytes > MAX_BYTES {
            return Err(CoreError::Integrity);
        }
        if let Some(env) = &file.env
            && [&env.client, &env.server]
                .iter()
                .any(|v| !matches!(v.as_str(), "required" | "optional" | "unsupported"))
        {
            return Err(CoreError::Integrity);
        }
    }
    reject_file_parents(seen.iter().map(String::as_str))?;
    Ok((request, version))
}
pub fn reject_file_parents<'a>(names: impl Iterator<Item = &'a str>) -> Result<(), CoreError> {
    let names: HashSet<String> = names.map(str::to_lowercase).collect();
    for name in &names {
        let mut prefix = String::new();
        let parts: Vec<_> = name.split('/').collect();
        for part in &parts[..parts.len().saturating_sub(1)] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            if names.contains(&prefix) {
                return Err(CoreError::UnsafeArchive);
            }
        }
    }
    Ok(())
}
pub fn digest(file: &Path) -> Result<EmbeddedFile, CoreError> {
    no_links(file)?;
    let input = File::open(file)?;
    let size = input.metadata()?.len();
    if size > MAX_FILE {
        return Err(CoreError::Integrity);
    }
    let mut hash = sha2::Sha512::new();
    let mut bytes = 0;
    let mut reader = input.take(MAX_FILE + 1);
    let mut buffer = [0; 65536];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        hash.update(&buffer[..count]);
    }
    if bytes != size {
        return Err(CoreError::SourceChanged);
    }
    Ok(EmbeddedFile {
        sha512: hex(&hash.finalize()),
        size,
    })
}
pub struct Parsed {
    pub manifest: Manifest,
    pub embedded: BTreeMap<String, EmbeddedFile>,
    pub warnings: Vec<String>,
}
pub fn parse(
    paths: &Paths,
    source: &Path,
    destination: &Path,
    sporium: bool,
) -> Result<Parsed, CoreError> {
    no_links(source)?;
    if !source.is_absolute() || source.metadata()?.len() > MAX_FILE {
        return Err(CoreError::UnsafeArchive);
    }
    let mut zip =
        zip::ZipArchive::new(File::open(source)?).map_err(|_| CoreError::UnsafeArchive)?;
    if zip.len() > MAX_FILES {
        return Err(CoreError::UnsafeArchive);
    }
    let index = if sporium {
        "sporium.index.json"
    } else {
        "modrinth.index.json"
    };
    let mut seen = HashSet::new();
    let mut ordinary_files = vec![];
    let mut expanded = 0u64;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|_| CoreError::Integrity)?;
        let name = entry.name().trim_end_matches('/');
        relative(name)?;
        if name
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
            || !seen.insert(name.to_lowercase())
            || entry
                .unix_mode()
                .is_some_and(|m| !matches!(m & 0o170000, 0 | 0o100000 | 0o040000))
        {
            return Err(CoreError::UnsafeArchive);
        }
        expanded = expanded
            .checked_add(entry.size())
            .ok_or(CoreError::Integrity)?;
        if expanded > MAX_BYTES || entry.size() > MAX_FILE {
            return Err(CoreError::UnsafeArchive);
        }
        if !entry.is_dir() {
            ordinary_files.push(name.to_string());
        }
    }
    reject_file_parents(ordinary_files.iter().map(String::as_str))?;
    let mut entry = zip.by_name(index).map_err(|_| CoreError::InvalidInput)?;
    if entry.size() > 12_000_000 {
        return Err(CoreError::Integrity);
    }
    let mut bytes = vec![];
    entry.by_ref().take(12_000_001).read_to_end(&mut bytes)?;
    drop(entry);
    let (manifest, declared) = if sporium {
        let pack: SporiumPack = serde_json::from_slice(&bytes)?;
        if pack.schema_version != 1 {
            return Err(CoreError::SchemaTooNew);
        }
        (pack.manifest, Some(pack.overrides))
    } else {
        (serde_json::from_slice::<Manifest>(&bytes)?, None)
    };
    metadata(&manifest)?;
    let mut embedded = BTreeMap::new();
    let mut warnings = vec![];
    // Overrides replace declared payloads; client layer replaces shared overrides.
    for layer in ["overrides/", "client-overrides/"] {
        if sporium && layer != "overrides/" {
            continue;
        }
        for name in &ordinary_files {
            let Some(target) = name.strip_prefix(layer) else {
                continue;
            };
            allowed_path(target)?;
            let mut entry = zip.by_name(name).map_err(|_| CoreError::Integrity)?;
            let file = paths.checked(&destination.join(relative(target)?))?;
            paths.mkdir(file.parent().ok_or(CoreError::UnsafePath)?)?;
            let mut temporary =
                tempfile::NamedTempFile::new_in(file.parent().ok_or(CoreError::UnsafePath)?)?;
            let size = entry.size();
            if std::io::copy(&mut entry.by_ref().take(size + 1), &mut temporary)? != size {
                return Err(CoreError::Integrity);
            }
            temporary.as_file().sync_all()?;
            temporary
                .persist(&file)
                .map_err(|e| CoreError::Io(e.error))?;
            // Case aliases across layers must not become two files on other operating systems.
            if let Some(previous) = embedded
                .keys()
                .find(|k: &&String| k.eq_ignore_ascii_case(target))
                .cloned()
                && previous != target
            {
                return Err(CoreError::UnsafeArchive);
            }
            embedded.insert(target.to_string(), digest(&file)?);
        }
    }
    if ordinary_files
        .iter()
        .any(|n| n.starts_with("server-overrides/"))
    {
        warnings.push("server_overrides_skipped".into());
    }
    let unknown = ordinary_files
        .iter()
        .filter(|n| {
            n.as_str() != index
                && !n.starts_with("overrides/")
                && !n.starts_with("client-overrides/")
                && !n.starts_with("server-overrides/")
        })
        .count();
    if unknown > 0 {
        warnings.push(format!("archive_metadata_skipped:{unknown}"));
    }
    if let Some(declared) = declared {
        if declared.len() != embedded.len() {
            return Err(CoreError::Integrity);
        }
        for (name, expected) in declared {
            allowed_path(&name)?;
            if !hash_valid(&expected.sha512, 128)
                || !verify(
                    &destination.join(&name),
                    &Hash::Sha512(expected.sha512),
                    expected.size,
                )?
            {
                return Err(CoreError::Integrity);
            }
        }
    }
    // Any file/parent collision across downloads and override layers fails before a download.
    let names: Vec<_> = manifest
        .files
        .iter()
        .map(|f| f.path.as_str())
        .chain(embedded.keys().map(String::as_str))
        .collect();
    reject_file_parents(names.into_iter())?;
    for file in &manifest.files {
        if let Some(name) = embedded.keys().find(|n| n.eq_ignore_ascii_case(&file.path))
            && name != &file.path
        {
            return Err(CoreError::UnsafeArchive);
        }
    }
    Ok(Parsed {
        manifest,
        embedded,
        warnings,
    })
}

pub fn zip_file(
    writer: &mut zip::ZipWriter<File>,
    name: &str,
    file: &Path,
) -> Result<(), CoreError> {
    writer
        .start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .map_err(|_| CoreError::Integrity)?;
    let mut input = File::open(file)?;
    std::io::copy(&mut input, writer)?;
    writer.flush()?;
    Ok(())
}
