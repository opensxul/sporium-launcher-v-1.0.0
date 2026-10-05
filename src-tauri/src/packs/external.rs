//! Read-only metadata adapters. Custom launch hooks, accounts and runtimes are never copied.
use super::{
    archive::{self, MAX_BYTES, MAX_FILES},
    model::*,
};
use crate::{
    error::CoreError,
    game::fs::relative,
    instances::filesystem::{Paths, no_links},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Candidate {
    pub view: ExternalCandidate,
    pub game: PathBuf,
    pub proofs: Vec<(PathBuf, EmbeddedFile)>,
}
pub fn verify_candidate(candidate: &Candidate) -> Result<(), CoreError> {
    for (path, expected) in &candidate.proofs {
        let actual = archive::digest(path)?;
        if actual.size != expected.size || actual.sha512 != expected.sha512 {
            return Err(CoreError::SourceChanged);
        }
        let wal = path.with_file_name(format!(
            "{}-wal",
            path.file_name()
                .ok_or(CoreError::UnsafePath)?
                .to_string_lossy()
        ));
        no_links(&wal)?;
        if path.extension().is_some_and(|s| s == "db") && wal.metadata().is_ok_and(|m| m.len() > 0)
        {
            return Err(CoreError::SourceChanged);
        }
    }
    Ok(())
}
fn proof(mut candidate: Candidate, paths: &[PathBuf]) -> Result<Candidate, CoreError> {
    for path in paths {
        candidate
            .proofs
            .push((path.clone(), archive::digest(path)?));
    }
    Ok(candidate)
}
fn read(path: &Path) -> Result<Vec<u8>, CoreError> {
    no_links(path)?;
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > 12_000_000 {
        return Err(CoreError::Integrity);
    }
    let mut bytes = vec![];
    file.take(12_000_001).read_to_end(&mut bytes)?;
    if bytes.len() > 12_000_000 {
        return Err(CoreError::Integrity);
    }
    Ok(bytes)
}
fn json(path: &Path) -> Result<Value, CoreError> {
    Ok(serde_json::from_slice(&read(path)?)?)
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, CoreError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or(CoreError::ContentUnsupported)
}
fn candidate(
    name: String,
    minecraft: String,
    loader: &str,
    loader_version: Option<String>,
    source: &str,
    game: PathBuf,
) -> Result<Candidate, CoreError> {
    if !game.is_absolute() {
        return Err(CoreError::UnsafePath);
    }
    no_links(&game)?;
    if !game.is_dir() {
        return Err(CoreError::NotFound);
    }
    let loader = match loader.to_ascii_lowercase().as_str() {
        "vanilla" => crate::instances::model::Loader::Vanilla,
        "fabric" => crate::instances::model::Loader::Fabric,
        "forge" => crate::instances::model::Loader::Forge,
        "neoforge" => crate::instances::model::Loader::NeoForge,
        _ => return Err(CoreError::ContentUnsupported),
    };
    if loader != crate::instances::model::Loader::Vanilla && loader_version.is_none() {
        return Err(CoreError::ContentUnsupported);
    }
    let view = ExternalCandidate {
        key: uuid::Uuid::new_v4().to_string(),
        name,
        minecraft,
        loader,
        loader_version,
        source: source.into(),
        warnings: vec!["external_settings_skipped".into()],
    };
    let m = manifest(&view);
    archive::metadata(&m)?;
    Ok(Candidate {
        view,
        game,
        proofs: vec![],
    })
}
pub fn manifest(candidate: &ExternalCandidate) -> Manifest {
    let mut dependencies = BTreeMap::from([("minecraft".into(), candidate.minecraft.clone())]);
    if let Some(version) = &candidate.loader_version {
        dependencies.insert(
            match candidate.loader {
                crate::instances::model::Loader::Fabric => "fabric-loader",
                crate::instances::model::Loader::Forge => "forge",
                crate::instances::model::Loader::NeoForge => "neoforge",
                _ => "unsupported",
            }
            .into(),
            version.clone(),
        );
    }
    Manifest {
        format_version: 1,
        game: "minecraft".into(),
        version_id: "external-copy".into(),
        name: candidate.name.clone(),
        summary: None,
        dependencies,
        files: vec![],
    }
}
fn prism(root: &Path) -> Result<Candidate, CoreError> {
    let pack = json(&root.join("mmc-pack.json"))?;
    if pack.get("formatVersion").and_then(Value::as_u64) != Some(1) {
        return Err(CoreError::SchemaTooNew);
    }
    for folder in ["patches", "jarmods", "instMods"] {
        let path = root.join(folder);
        no_links(&path)?;
        if path.is_dir() && fs::read_dir(path)?.next().is_some() {
            return Err(CoreError::ContentUnsupported);
        }
    }
    let components = pack
        .get("components")
        .and_then(Value::as_array)
        .filter(|a| a.len() <= 8)
        .ok_or(CoreError::ContentUnsupported)?;
    let mut minecraft = None;
    let mut loader = "vanilla";
    let mut version = None;
    let mut seen = std::collections::HashSet::new();
    for component in components {
        let uid = string(component, "uid")?;
        if !seen.insert(uid) {
            return Err(CoreError::DependencyConflict);
        }
        let v = string(component, "version")?;
        match uid {
            "net.minecraft" => minecraft = Some(v.to_string()),
            "org.lwjgl" | "org.lwjgl3" | "net.minecraftforge.fml" => (),
            "net.fabricmc.fabric-loader" | "net.minecraftforge" | "net.neoforged" => {
                if version.is_some() {
                    return Err(CoreError::DependencyConflict);
                }
                loader = match uid {
                    "net.fabricmc.fabric-loader" => "fabric",
                    "net.minecraftforge" => "forge",
                    _ => "neoforge",
                };
                version = Some(v.to_string());
            }
            _ => return Err(CoreError::ContentUnsupported),
        }
    }
    let minecraft = minecraft.ok_or(CoreError::ContentUnsupported)?;
    if loader == "forge"
        && let Some(v) = &mut version
        && let Some(suffix) = v.strip_prefix(&format!("{minecraft}-"))
    {
        *v = suffix.to_string();
    }
    let cfg =
        String::from_utf8(read(&root.join("instance.cfg"))?).map_err(|_| CoreError::Integrity)?;
    let name = cfg
        .lines()
        .find_map(|line| line.strip_prefix("name="))
        .filter(|s| !s.is_empty())
        .ok_or(CoreError::ContentUnsupported)?;
    let games: Vec<_> = [root.join(".minecraft"), root.join("minecraft")]
        .into_iter()
        .filter(|p| p.is_dir())
        .collect();
    if games.len() != 1 {
        return Err(CoreError::ContentUnsupported);
    }
    proof(
        candidate(
            name.into(),
            minecraft,
            loader,
            version,
            "Prism / MultiMC",
            games[0].clone(),
        )?,
        &[root.join("mmc-pack.json"), root.join("instance.cfg")],
    )
}
fn atlauncher(root: &Path) -> Result<Candidate, CoreError> {
    let value = json(&root.join("instance.json"))?;
    let launcher = value.get("launcher").ok_or(CoreError::ContentUnsupported)?;
    let loader = launcher.get("loaderVersion").filter(|v| !v.is_null());
    let (family, version) = if let Some(loader) = loader {
        (
            string(loader, "type")?,
            Some(string(loader, "version")?.to_string()),
        )
    } else {
        ("vanilla", None)
    };
    let minecraft = if loader.is_some() {
        string(&value, "inheritsFrom")?
    } else {
        string(&value, "id")?
    };
    proof(
        candidate(
            string(launcher, "name")?.into(),
            minecraft.into(),
            family,
            version,
            "ATLauncher",
            root.to_path_buf(),
        )?,
        &[root.join("instance.json")],
    )
}
pub fn scan(root: &Path) -> Result<Vec<Candidate>, CoreError> {
    if !root.is_absolute() {
        return Err(CoreError::UnsafePath);
    }
    no_links(root)?;
    if !root.is_dir() {
        return Err(CoreError::NotFound);
    }
    if root.join("mmc-pack.json").is_file() {
        return prism(root).map(|v| vec![v]);
    }
    if root.join("instance.json").is_file() {
        return atlauncher(root).map(|v| vec![v]);
    }
    if root.join("launcher_profiles.json").is_file() {
        let value = json(&root.join("launcher_profiles.json"))?;
        let profiles = value
            .get("profiles")
            .and_then(Value::as_object)
            .filter(|v| v.len() <= 500)
            .ok_or(CoreError::ContentUnsupported)?;
        let mut result = vec![];
        for (id, p) in profiles {
            let Some(version) = p.get("lastVersionId").and_then(Value::as_str) else {
                continue;
            };
            // Only official, self-contained version metadata is safely mapped; custom launchers use their own loaders.
            if version.starts_with("latest-") {
                continue;
            }
            let game = p
                .get("gameDir")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .unwrap_or_else(|| root.to_path_buf());
            let metadata = root
                .join("versions")
                .join(relative(version)?)
                .join(format!("{version}.json"));
            if !metadata.is_file() {
                continue;
            }
            let v = json(&metadata)?;
            if v.get("inheritsFrom").is_some()
                || v.get("id").and_then(Value::as_str) != Some(version)
                || v.get("mainClass").and_then(Value::as_str)
                    != Some("net.minecraft.client.main.Main")
            {
                continue;
            }
            result.push(proof(
                candidate(
                    p.get("name").and_then(Value::as_str).unwrap_or(id).into(),
                    version.into(),
                    "vanilla",
                    None,
                    "Minecraft Launcher",
                    game,
                )?,
                &[root.join("launcher_profiles.json"), metadata],
            )?);
        }
        if result.is_empty() {
            return Err(CoreError::ContentUnsupported);
        }
        return Ok(result);
    }
    // Modrinth's current app uses SQLite. Copying a closed database avoids changing WAL/SHM in the source.
    for name in ["app.db", "app.sqlite", "database.db"] {
        let db = root.join(name);
        if !db.is_file() {
            continue;
        }
        no_links(&db)?;
        let wal = db.with_file_name(format!("{name}-wal"));
        no_links(&wal)?;
        if wal.metadata().is_ok_and(|m| m.len() > 0) {
            return Err(CoreError::SourceChanged);
        }
        let temp = tempfile::NamedTempFile::new()?;
        if db.metadata()?.len() > 500_000_000 {
            return Err(CoreError::Integrity);
        }
        let before = archive::digest(&db)?;
        fs::copy(&db, temp.path())?;
        let after = archive::digest(&db)?;
        let copied = archive::digest(temp.path())?;
        if before.sha512 != after.sha512
            || before.sha512 != copied.sha512
            || wal.metadata().is_ok_and(|m| m.len() > 0)
        {
            return Err(CoreError::SourceChanged);
        }
        let connection = rusqlite::Connection::open_with_flags(
            temp.path(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let current = connection.prepare("SELECT i.name, i.path, c.game_version, c.loader, c.loader_version FROM instances i JOIN instance_content_sets c ON c.id=i.applied_content_set_id AND c.instance_id=i.id WHERE i.install_stage='installed' LIMIT 501");
        let mut query = match current { Ok(q)=>q, Err(_)=>connection.prepare("SELECT name, path, game_version, mod_loader, mod_loader_version FROM profiles WHERE install_stage='installed' LIMIT 501").map_err(|_|CoreError::ContentUnsupported)? };
        let rows = query
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.len() > 500 {
            return Err(CoreError::Integrity);
        }
        let mut result = vec![];
        for (name, path, game, loader, version) in rows {
            if relative(&path)?.components().count() != 1 {
                return Err(CoreError::UnsafePath);
            }
            result.push(proof(
                candidate(
                    name,
                    game,
                    &loader,
                    version,
                    "Modrinth App",
                    root.join("profiles").join(path),
                )?,
                std::slice::from_ref(&db),
            )?);
        }
        if result.is_empty() {
            return Err(CoreError::ContentUnsupported);
        }
        return Ok(result);
    }
    // A launcher instances directory: inspect only immediate known metadata, never guess arbitrary game dirs.
    let mut result = vec![];
    for (count, entry) in fs::read_dir(root)?.enumerate() {
        if count >= 500 {
            return Err(CoreError::Integrity);
        }
        let path = entry?.path();
        no_links(&path)?;
        if path.is_dir() && path.join("mmc-pack.json").is_file() {
            result.push(prism(&path)?);
        } else if path.is_dir() && path.join("instance.json").is_file() {
            result.push(atlauncher(&path)?);
        }
    }
    if result.is_empty() {
        return Err(CoreError::ContentUnsupported);
    }
    Ok(result)
}
fn walk(
    root: &Path,
    current: &Path,
    files: &mut Vec<(String, PathBuf)>,
    count: &mut usize,
    depth: usize,
) -> Result<(), CoreError> {
    no_links(current)?;
    *count += 1;
    if *count > MAX_FILES || depth > 32 {
        return Err(CoreError::Integrity);
    }
    if current.is_file() {
        let name = current
            .strip_prefix(root)
            .map_err(|_| CoreError::UnsafePath)?
            .to_string_lossy()
            .replace('\\', "/");
        let name = if let Some(file) = name.strip_prefix("disabledmods/") {
            format!("mods_disabled/{file}")
        } else if name.starts_with("mods/")
            && name.ends_with(".jar.disabled")
            && name.split('/').count() == 2
        {
            format!(
                "mods_disabled/{}",
                name.strip_prefix("mods/")
                    .ok_or(CoreError::UnsafePath)?
                    .trim_end_matches(".disabled")
            )
        } else {
            name
        };
        archive::allowed_path(&name)?;
        files.push((name, current.to_path_buf()));
    } else if current.is_dir() {
        for entry in fs::read_dir(current)? {
            walk(root, &entry?.path(), files, count, depth + 1)?;
        }
    } else {
        return Err(CoreError::UnsafePath);
    }
    Ok(())
}
pub type SourceFiles = (Vec<(String, PathBuf)>, Vec<String>);
pub fn files(root: &Path, include_worlds: bool) -> Result<SourceFiles, CoreError> {
    no_links(root)?;
    let mut files = vec![];
    let mut omitted = vec![];
    let mut count = 0;
    for (i, entry) in fs::read_dir(root)?.enumerate() {
        if i >= MAX_FILES {
            return Err(CoreError::Integrity);
        }
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| CoreError::UnsafePath)?;
        if (archive::allowed_path(&name).is_ok() || name == "disabledmods")
            && (include_worlds || name != "saves")
        {
            walk(root, &entry.path(), &mut files, &mut count, 0)?;
        } else {
            omitted.push(name);
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut seen = std::collections::HashSet::new();
    let mut bytes = 0u64;
    for (name, path) in &files {
        if !seen.insert(name.to_lowercase()) {
            return Err(CoreError::UnsafePath);
        }
        bytes = bytes
            .checked_add(path.metadata()?.len())
            .ok_or(CoreError::Integrity)?;
        if bytes > MAX_BYTES {
            return Err(CoreError::Integrity);
        }
    }
    Ok((files, omitted))
}
pub fn copy(
    paths: &Paths,
    source: &Path,
    target: &Path,
) -> Result<(BTreeMap<String, EmbeddedFile>, Vec<String>), CoreError> {
    let (files, omitted) = files(source, true)?;
    let mut embedded = BTreeMap::new();
    for (name, file) in files {
        let before = archive::digest(&file)?;
        let destination = paths.checked(&target.join(&name))?;
        paths.mkdir(destination.parent().ok_or(CoreError::UnsafePath)?)?;
        fs::copy(&file, &destination)?;
        let after = archive::digest(&file)?;
        let copied = archive::digest(&destination)?;
        if before.sha512 != after.sha512
            || before.sha512 != copied.sha512
            || before.size != copied.size
        {
            return Err(CoreError::SourceChanged);
        }
        fs::OpenOptions::new()
            .write(true)
            .open(destination)?
            .sync_all()?;
        embedded.insert(name, copied);
    }
    Ok((embedded, omitted))
}
