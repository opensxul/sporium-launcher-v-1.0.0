use super::{install, model::*, resolve};
use crate::{
    error::CoreError,
    instances::{
        filesystem::{Paths, no_links},
        model::{Instance, Loader},
    },
};
use sha1::Digest;
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_JAR: u64 = 200_000_000;

fn numbers(value: &str) -> Option<Vec<u32>> {
    let parts: Vec<_> = value
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    (!parts.is_empty() && parts.len() <= 6).then_some(parts)
}
fn compare(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let mut a = numbers(a)?;
    let mut b = numbers(b)?;
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    Some(a.cmp(&b))
}
/// Only evaluate understood numeric predicates. Snapshot and custom syntax remains unknown.
pub(super) fn matches_version(current: &str, expression: &str) -> Option<bool> {
    let expression = expression.trim();
    if expression == "*" || expression.is_empty() {
        return Some(true);
    }
    if expression.starts_with(['[', '(']) && expression.ends_with([']', ')']) {
        let body = &expression[1..expression.len() - 1];
        if let Some((lo, hi)) = body.split_once(',') {
            if hi.contains(',') {
                return None;
            }
            let lower = lo.trim().is_empty()
                || match compare(current, lo.trim())? {
                    std::cmp::Ordering::Greater => true,
                    std::cmp::Ordering::Equal => expression.starts_with('['),
                    _ => false,
                };
            let upper = hi.trim().is_empty()
                || match compare(current, hi.trim())? {
                    std::cmp::Ordering::Less => true,
                    std::cmp::Ordering::Equal => expression.ends_with(']'),
                    _ => false,
                };
            return Some(lower && upper);
        }
        return if expression.starts_with('[') && expression.ends_with(']') {
            Some(compare(current, body)? == std::cmp::Ordering::Equal)
        } else {
            None
        };
    }
    let mut matched = true;
    for term in expression.split_whitespace() {
        let (op, value) = [">=", "<=", ">", "<", "="]
            .iter()
            .find_map(|op| term.strip_prefix(op).map(|v| (*op, v)))
            .unwrap_or(("=", term));
        if op == "=" && (value.ends_with(".*") || value.ends_with(".x")) {
            let prefix = numbers(&value[..value.len() - 2])?;
            matched &= numbers(current)?.starts_with(&prefix);
        } else {
            let cmp = compare(current, value)?;
            matched &= match op {
                ">=" => !cmp.is_lt(),
                "<=" => !cmp.is_gt(),
                ">" => cmp.is_gt(),
                "<" => cmp.is_lt(),
                _ => cmp.is_eq(),
            };
        }
    }
    Some(matched)
}
fn check_ranges(
    current: &str,
    ranges: &[String],
    warnings: &mut Vec<String>,
) -> Result<(), CoreError> {
    if ranges.is_empty() {
        warnings.push("unknown_version".into());
        return Ok(());
    }
    let matches: Vec<_> = ranges.iter().map(|s| matches_version(current, s)).collect();
    if matches.contains(&Some(true)) {
        return Ok(());
    }
    if matches.iter().all(|v| *v == Some(false)) {
        return Err(CoreError::ContentIncompatible);
    }
    warnings.push("unknown_version".into());
    Ok(())
}
fn text<'a>(json: &'a serde_json::Value, key: &str) -> &'a str {
    json.get(key).and_then(|v| v.as_str()).unwrap_or("")
}
fn strings(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => vec![],
    }
}

// Fabric's bundled JsonReader accepts literal newlines inside quoted strings.
// Normalize only these string characters; structural JSON errors remain errors.
fn fabric_json(data: &str) -> Result<serde_json::Value, CoreError> {
    let mut normalized = String::with_capacity(data.len());
    let mut quoted = false;
    let mut escaped = false;
    for character in data.chars() {
        if escaped {
            normalized.push(character);
            escaped = false;
        } else if quoted && character == '\\' {
            normalized.push(character);
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
            normalized.push(character);
        } else if quoted && matches!(character, '\n' | '\r' | '\t') {
            normalized.push_str(match character {
                '\n' => "\\n",
                '\r' => "\\r",
                _ => "\\t",
            });
        } else {
            normalized.push(character);
        }
    }
    serde_json::from_str(&normalized).map_err(|_| CoreError::Integrity)
}

pub(super) fn inspect(
    path: &Path,
    instance: &Instance,
) -> Result<(String, String, LocalMetadata), CoreError> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_JAR {
        return Err(CoreError::Integrity);
    }
    let mut zip = zip::ZipArchive::new(file).map_err(|_| CoreError::Integrity)?;
    if zip.len() > 100_000 {
        return Err(CoreError::Integrity);
    }
    let names: Vec<_> = zip.file_names().map(String::from).collect();
    let mut read = |name: &str| -> Result<Option<String>, CoreError> {
        if !names.iter().any(|s| s == name) {
            return Ok(None);
        }
        if names.iter().filter(|s| s.as_str() == name).count() != 1 {
            return Err(CoreError::Integrity);
        }
        let entry = zip.by_name(name).map_err(|_| CoreError::Integrity)?;
        if entry.size() > 524_288 {
            return Err(CoreError::Integrity);
        }
        let mut bytes = vec![];
        entry.take(524_289).read_to_end(&mut bytes)?;
        if bytes.len() > 524_288 {
            return Err(CoreError::Integrity);
        }
        Ok(Some(
            String::from_utf8(bytes).map_err(|_| CoreError::Integrity)?,
        ))
    };
    let fabric = read("fabric.mod.json")?;
    let neo = read("META-INF/neoforge.mods.toml")?;
    let forge = read("META-INF/mods.toml")?;
    let legacy = read("mcmod.info")?;
    let mut meta = LocalMetadata::default();
    let mut title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Local mod")
        .to_string();
    let mut version = String::from("?");
    if instance.loader == Loader::Vanilla {
        return Err(CoreError::ContentIncompatible);
    }
    if instance.loader == Loader::Fabric
        && let Some(data) = &fabric
    {
        let json = fabric_json(data)?;
        if json.get("schemaVersion").and_then(|v| v.as_u64()) != Some(1) {
            meta.warnings.push("unknown_metadata".into());
        }
        if text(&json, "environment") == "server" {
            return Err(CoreError::ContentIncompatible);
        }
        let id = text(&json, "id");
        if id.is_empty() {
            return Err(CoreError::Integrity);
        }
        meta.mod_ids.push(id.into());
        meta.mod_ids.extend(strings(json.get("provides")));
        meta.loader = "fabric".into();
        title = if text(&json, "name").is_empty() {
            id
        } else {
            text(&json, "name")
        }
        .into();
        version = text(&json, "version").into();
        meta.versions.push(LocalModVersion {
            id: id.into(),
            version: known_version(&version),
            alias: false,
        });
        for alias in meta.mod_ids.iter().skip(1) {
            meta.versions.push(LocalModVersion {
                id: alias.clone(),
                version: None,
                alias: true,
            });
        }
        for (key, relation) in [
            ("depends", "required"),
            ("recommends", "recommended"),
            ("suggests", "optional"),
            ("conflicts", "conflict"),
            ("breaks", "breaks"),
        ] {
            if let Some(map) = json.get(key).and_then(|v| v.as_object()) {
                for (target, predicate) in map {
                    meta.dependencies.push(LocalDependency {
                        owner_id: id.into(),
                        id: target.clone(),
                        relation: relation.into(),
                        ranges: strings(Some(predicate)),
                        dialect: "fabric".into(),
                    });
                }
            }
        }
        if let Some(depends) = json.get("depends").and_then(|v| v.as_object()) {
            meta.minecraft = strings(depends.get("minecraft"));
            for (id, predicate) in depends {
                if id == "fabricloader" {
                    if let Some(v) = &instance.loader_version {
                        check_ranges(v, &strings(Some(predicate)), &mut meta.warnings)?;
                    } else {
                        meta.warnings.push("unknown_loader_version".into());
                    }
                } else if id != "minecraft" {
                    meta.required.push(id.clone());
                }
            }
        }
        if json
            .get("jars")
            .and_then(|v| v.as_array())
            .is_some_and(|v| !v.is_empty())
        {
            meta.warnings.push("nested_mods".into());
        }
    } else if matches!(instance.loader, Loader::Forge | Loader::NeoForge)
        && (neo.is_some() || forge.is_some())
    {
        let data = if instance.loader == Loader::NeoForge {
            neo.as_ref().or(forge.as_ref())
        } else {
            forge.as_ref()
        }
        .ok_or(CoreError::ContentIncompatible)?;
        let value: toml::Value = toml::from_str(data).map_err(|_| CoreError::Integrity)?;
        let mods = value
            .get("mods")
            .and_then(|v| v.as_array())
            .ok_or(CoreError::Integrity)?;
        let mut dependencies = vec![];
        if let Some(map) = value.get("dependencies").and_then(|v| v.as_table()) {
            for (owner, list) in map {
                if let Some(list) = list.as_array() {
                    dependencies.extend(list.iter().map(|dep| (owner, dep)));
                }
            }
        }
        let is_neo = neo.as_ref() == Some(data)
            || dependencies
                .iter()
                .any(|(_, d)| d.get("modId").and_then(|v| v.as_str()) == Some("neoforge"));
        meta.loader = if is_neo { "neoforge" } else { "forge" }.into();
        if meta.loader != resolve::loader_name(instance.loader) {
            return Err(CoreError::ContentIncompatible);
        }
        for item in mods {
            let id = item
                .get("modId")
                .and_then(|v| v.as_str())
                .ok_or(CoreError::Integrity)?;
            meta.mod_ids.push(id.into());
            meta.versions.push(LocalModVersion {
                id: id.into(),
                version: item
                    .get("version")
                    .and_then(|v| v.as_str())
                    .and_then(known_version),
                alias: false,
            });
        }
        if let Some(first) = mods.first() {
            title = first
                .get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or(&title)
                .into();
            version = first
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .into();
        }
        for (owner, dep) in dependencies {
            if dep.get("side").and_then(|v| v.as_str()) == Some("SERVER") {
                continue;
            }
            let unknown_side = dep
                .get("side")
                .and_then(|v| v.as_str())
                .is_some_and(|s| !matches!(s, "BOTH" | "CLIENT"));
            let id = dep.get("modId").and_then(|v| v.as_str()).unwrap_or("");
            let required = dep
                .get("mandatory")
                .and_then(|v| v.as_bool())
                .unwrap_or_else(|| {
                    dep.get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("required")
                        == "required"
                });
            let range = dep
                .get("versionRange")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let relation = match dep.get("type").and_then(|v| v.as_str()) {
                Some("incompatible") => "breaks",
                Some("discouraged") => "conflict",
                Some(kind) if !matches!(kind, "required" | "optional") => "unknown",
                _ if required => "required",
                _ => "optional",
            };
            if unknown_side || relation == "unknown" {
                meta.warnings.push("unknown_dependency".into());
            }
            if !id.is_empty() {
                meta.dependencies.push(LocalDependency {
                    owner_id: owner.clone(),
                    id: id.into(),
                    relation: if unknown_side { "unknown" } else { relation }.into(),
                    ranges: vec![range.clone()],
                    dialect: "maven".into(),
                });
            }
            if unknown_side || relation != "required" {
                continue;
            }
            if id == "minecraft" {
                check_ranges(
                    &instance.minecraft_version,
                    std::slice::from_ref(&range),
                    &mut meta.warnings,
                )?;
                meta.minecraft.push(range);
            } else if id == meta.loader {
                if let Some(v) = &instance.loader_version {
                    check_ranges(v, &[range], &mut meta.warnings)?;
                }
            } else if !id.is_empty() {
                meta.required.push(id.into());
            }
        }
        // FML's language-loader version and entrypoint side cannot be inferred from dependency side.
        meta.warnings.push("unknown_environment".into());
    } else if instance.loader == Loader::Forge && legacy.is_some() {
        let json: serde_json::Value = serde_json::from_str(legacy.as_deref().unwrap_or(""))
            .map_err(|_| CoreError::Integrity)?;
        let mods = json
            .as_array()
            .or_else(|| json.get("modList").and_then(|v| v.as_array()))
            .ok_or(CoreError::Integrity)?;
        for item in mods {
            let id = text(item, "modid");
            if !id.is_empty() {
                meta.mod_ids.push(id.into());
                meta.versions.push(LocalModVersion {
                    id: id.into(),
                    version: known_version(text(item, "version")),
                    alias: false,
                });
            }
        }
        if let Some(item) = mods.first() {
            title = text(item, "name").into();
            version = text(item, "version").into();
        }
        meta.loader = "forge".into();
        meta.warnings.push("unknown_metadata".into());
    } else if fabric.is_some()
        || neo.is_some()
        || forge.is_some()
        || legacy.is_some()
        || names.iter().any(|s| {
            matches!(
                s.as_str(),
                "quilt.mod.json" | "plugin.yml" | "paper-plugin.yml"
            )
        })
    {
        return Err(CoreError::ContentIncompatible);
    } else {
        meta.warnings.push("unknown_metadata".into());
    }
    if meta.loader == "fabric" {
        check_ranges(
            &instance.minecraft_version,
            &meta.minecraft,
            &mut meta.warnings,
        )?;
    } else if meta.minecraft.is_empty() {
        meta.warnings.push("unknown_version".into());
    }
    if meta.mod_ids.len() > 128
        || meta.mod_ids.iter().any(|id| {
            id.is_empty()
                || id.len() > 128
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        })
    {
        return Err(CoreError::Integrity);
    }
    if meta.dependencies.len() > 512
        || meta.dependencies.iter().any(|d| {
            !valid_mod_id(&d.id)
                || !meta.mod_ids.contains(&d.owner_id)
                || d.ranges.len() > 32
                || d.ranges.iter().any(|r| r.len() > 256)
                || d.ranges.iter().map(String::len).sum::<usize>() > 2048
        })
    {
        return Err(CoreError::Integrity);
    }
    meta.warnings.sort();
    meta.warnings.dedup();
    Ok((
        title.chars().take(160).collect(),
        version.chars().take(160).collect(),
        meta,
    ))
}

fn known_version(value: &str) -> Option<String> {
    (!value.is_empty() && value.len() <= 160 && !value.contains("${") && value != "?")
        .then(|| value.to_string())
}
fn valid_mod_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}

pub(super) fn scan_ids(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
) -> Result<HashSet<String>, CoreError> {
    let mut ids = HashSet::new();
    let mut count = 0;
    for folder in ["mods", "mods_disabled"] {
        for entry in std::fs::read_dir(paths.checked(&directory.join(folder))?)? {
            let path = paths.checked(&entry?.path())?;
            if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jar"))
            {
                count += 1;
                if count > 4096 {
                    return Err(CoreError::Integrity);
                }
                // Unknown existing JARs remain untouched; their compatibility is never asserted.
                if let Ok((_, _, meta)) = inspect(&path, instance) {
                    ids.extend(meta.mod_ids);
                }
            }
        }
    }
    Ok(ids)
}

pub(super) fn prepare(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
    sources: Vec<String>,
) -> Result<LocalContentPlan, CoreError> {
    if sources.is_empty() || sources.len() > 64 {
        return Err(CoreError::InvalidInput);
    }
    let token = uuid::Uuid::new_v4().to_string();
    let result = (|| {
        let mut ids = scan_ids(paths, directory, instance)?;
        let mut names = HashSet::new();
        let mut files = vec![];
        let mut warnings = vec![];
        let mut total = 0;
        for (index, source) in sources.iter().enumerate() {
            let source = PathBuf::from(source);
            if !source.is_absolute() {
                return Err(CoreError::UnsafePath);
            }
            no_links(&source)?;
            let name = source
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or(CoreError::InvalidInput)?
                .to_string();
            crate::game::fs::relative(&name)?;
            if source
                .extension()
                .and_then(|s| s.to_str())
                .is_none_or(|s| !s.eq_ignore_ascii_case("jar"))
                || name.len() > 180
                || !names.insert(name.to_lowercase())
            {
                return Err(CoreError::InvalidInput);
            }
            let file = File::open(&source)?;
            let size = file.metadata()?.len();
            total += size;
            if !file.metadata()?.is_file() || size == 0 || size > MAX_JAR || total > 1_000_000_000 {
                return Err(CoreError::Integrity);
            }
            let staged = install::stage(paths, directory, &token, index)?;
            paths.mkdir(staged.parent().ok_or(CoreError::UnsafePath)?)?;
            let mut output = File::create(&staged)?;
            let copied = std::io::copy(&mut file.take(MAX_JAR + 1), &mut output)?;
            output.flush()?;
            output.sync_all()?;
            if copied != size {
                return Err(CoreError::Integrity);
            }
            let (title, version, meta) = inspect(&staged, instance)?;
            for id in &meta.mod_ids {
                if !ids.insert(id.clone()) {
                    return Err(CoreError::ContentConflict);
                }
            }
            warnings.extend(meta.warnings.iter().cloned());
            let mut file = File::open(&staged)?;
            let mut sha1 = sha1::Sha1::new();
            let mut sha512 = sha2::Sha512::new();
            let mut buffer = [0; 65536];
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                sha1.update(&buffer[..n]);
                sha512.update(&buffer[..n]);
            }
            let id = uuid::Uuid::new_v4().to_string();
            let artifact = ContentFile {
                id: None,
                filename: name.clone(),
                url: String::new(),
                hashes: ContentHashes {
                    sha1: crate::game::network::hex(&sha1.finalize()),
                    sha512: crate::game::network::hex(&sha512.finalize()),
                },
                size,
                primary: true,
                file_type: None,
            };
            let title = if title == format!("{index}") || title.is_empty() {
                name.clone()
            } else {
                title
            };
            files.push(ContentRecord {
                local: Some(meta.clone()),
                icon_url: None,
                provider: "local".into(),
                project_id: id.clone(),
                title,
                kind: "mod".into(),
                version: ContentVersion {
                    id: id.clone(),
                    project_id: id,
                    name: name.clone(),
                    version_number: version,
                    version_type: "local".into(),
                    date_published: String::new(),
                    status: "local".into(),
                    game_versions: vec![],
                    loaders: vec![meta.loader],
                    environment: "unknown".into(),
                    dependencies: vec![],
                    files: vec![artifact.clone()],
                },
                file: artifact,
                directory: "mods".into(),
                dependency: false,
            });
        }
        install::preflight(
            paths,
            directory,
            &files,
            &install::records(paths, directory)?,
        )?;
        let diagnostics = super::diagnostics::scan(paths, directory, instance, &files)?;
        if super::diagnostics::selected_issues(&diagnostics, &files) {
            warnings.push("dependency_issues".into());
        }
        warnings.sort();
        warnings.dedup();
        Ok(LocalContentPlan {
            diagnostics,
            plan: ContentPlan {
                new_instance: None,
                token: token.clone(),
                instance_id: instance.id.clone(),
                files,
                optional_dependencies: 0,
                already_installed: 0,
                total_bytes: total,
            },
            warnings,
        })
    })();
    if result.is_err() {
        let _ = paths.remove(&directory.join(".sporium/content-staging").join(token));
    }
    result
}
