//! Offline, read-only dependency checks against freshly inspected client JAR metadata.
use super::{local, model::*};
use crate::{
    error::CoreError,
    instances::{
        filesystem::Paths,
        model::{Instance, Loader},
    },
};
use std::{collections::HashMap, path::Path};

pub(super) fn matches(version: Option<&str>, ranges: &[String], dialect: &str) -> Option<bool> {
    if ranges.is_empty() {
        return None;
    }
    let values: Vec<_> = ranges
        .iter()
        .map(|r| predicate(version, r, dialect))
        .collect();
    if values.contains(&Some(true)) {
        Some(true)
    } else if values.iter().all(|v| *v == Some(false)) {
        Some(false)
    } else {
        None
    }
}
fn predicate(version: Option<&str>, range: &str, dialect: &str) -> Option<bool> {
    let range = range.trim();
    if range == "*" || (dialect == "maven" && range.is_empty()) {
        return Some(true);
    }
    let version = version?;
    if dialect == "maven" {
        return if range.starts_with(['[', '(']) && !version.contains(['+', '-']) {
            local::matches_version(version, range)
        } else {
            None
        };
    }
    if dialect != "fabric" || range.is_empty() {
        return None;
    }
    let current = version.split('+').next()?;
    let mut unknown = false;
    for term in range.split_whitespace() {
        let result = if let Some(base) = term.strip_prefix('^').or_else(|| term.strip_prefix('~')) {
            let mut upper: Vec<u32> = match base
                .split('.')
                .map(str::parse)
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(parts) if (2..=6).contains(&parts.len()) => parts,
                _ => {
                    unknown = true;
                    continue;
                }
            };
            upper.resize(3, 0);
            upper.truncate(3);
            let index = usize::from(term.starts_with('~'));
            upper[index] = upper[index].checked_add(1)?;
            for component in upper.iter_mut().skip(index + 1) {
                *component = 0;
            }
            let upper = upper
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(".");
            local::matches_version(current, &format!(">={base} <{upper}"))
        } else if term.contains(['[', '(', ',', '|']) {
            None
        } else {
            let term = term.replace(".X", ".x");
            let exact = term.strip_prefix('=').unwrap_or(&term);
            if !term.starts_with(['>', '<'])
                && !term.ends_with(['*', 'x'])
                && (version == exact || current == exact)
            {
                Some(true)
            } else {
                local::matches_version(current, &term)
            }
        };
        match result {
            Some(false) => return Some(false),
            None => unknown = true,
            _ => (),
        }
    }
    if unknown { None } else { Some(true) }
}

struct Node {
    row: ModDiagnostic,
    metadata: Option<LocalMetadata>,
}
pub(super) fn scan(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
    additions: &[ContentRecord],
) -> Result<ModDiagnostics, CoreError> {
    let mut nodes = vec![];
    let mut count = 0;
    for folder in ["mods", "mods_disabled"] {
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
            let path = paths.checked(&entry.path())?;
            if !path.is_file()
                || !path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("jar"))
            {
                continue;
            }
            let filename = entry.file_name().to_string_lossy().to_string();
            let mut row = ModDiagnostic {
                title: filename.clone(),
                directory: folder.into(),
                filename,
                enabled: folder == "mods",
                status: "readable".into(),
                warnings: vec![],
                checks: vec![],
            };
            let metadata = match local::inspect(&path, instance) {
                Ok((title, _, metadata)) => {
                    row.title = title;
                    Some(metadata)
                }
                Err(CoreError::ContentIncompatible) => {
                    row.status = "incompatible".into();
                    None
                }
                Err(_) => {
                    row.status = "unreadable".into();
                    None
                }
            };
            nodes.push(Node { row, metadata });
        }
    }
    for record in additions {
        if record.kind != "mod"
            || nodes.iter().any(|n| {
                n.row.directory == record.directory
                    && n.row.filename.eq_ignore_ascii_case(&record.file.filename)
            })
        {
            continue;
        }
        nodes.push(Node {
            row: ModDiagnostic {
                title: record.title.clone(),
                directory: record.directory.clone(),
                filename: record.file.filename.clone(),
                enabled: record.directory == "mods",
                status: "readable".into(),
                warnings: vec![],
                checks: vec![],
            },
            metadata: record.local.clone(),
        });
    }
    nodes.sort_by(|a, b| {
        (a.row.directory.as_str(), a.row.filename.to_lowercase())
            .cmp(&(b.row.directory.as_str(), b.row.filename.to_lowercase()))
    });
    let mut complete = true;
    let mut index: HashMap<String, Vec<(usize, LocalModVersion)>> = HashMap::new();
    let mut edges = 0;
    for (i, node) in nodes.iter_mut().enumerate() {
        if let Some(meta) = &node.metadata {
            node.row.warnings = meta
                .warnings
                .iter()
                .filter(|w| w.as_str() != "dependencies_unverified")
                .cloned()
                .collect();
            if meta
                .warnings
                .iter()
                .any(|w| w == "unknown_metadata" || w == "nested_mods")
            {
                if node.row.enabled {
                    complete = false;
                }
                if meta.warnings.iter().any(|w| w == "unknown_metadata") {
                    node.row.status = "unknown".into();
                }
            }
            edges += meta.dependencies.len();
            if edges > 16384 {
                return Err(CoreError::Integrity);
            }
            for version in &meta.versions {
                index
                    .entry(version.id.clone())
                    .or_default()
                    .push((i, version.clone()));
            }
        } else if node.row.enabled {
            complete = false;
        }
    }
    let mut checks: Vec<Vec<DependencyCheck>> = vec![vec![]; nodes.len()];
    let mut duplicate = vec![false; nodes.len()];
    for candidates in index.values() {
        let active: Vec<_> = candidates
            .iter()
            .filter(|(i, v)| nodes[*i].row.enabled && !v.alias)
            .collect();
        if active.len() > 1 {
            for (i, _) in active {
                duplicate[*i] = true;
            }
        }
    }
    for (i, node) in nodes.iter().enumerate() {
        let Some(meta) = &node.metadata else {
            continue;
        };
        for dep in &meta.dependencies {
            let candidates = index.get(&dep.id).cloned().unwrap_or_default();
            let mut active: Vec<_> = candidates
                .iter()
                .filter(|(i, _)| nodes[*i].row.enabled)
                .collect();
            if active.iter().any(|(_, v)| !v.alias) {
                active.retain(|(_, v)| !v.alias);
            }
            let builtin = match dep.id.as_str() {
                "minecraft" => Some(Some(instance.minecraft_version.as_str())),
                "java" => Some(None),
                "fabricloader" if instance.loader == Loader::Fabric => {
                    Some(instance.loader_version.as_deref())
                }
                "forge" if instance.loader == Loader::Forge => {
                    Some(instance.loader_version.as_deref())
                }
                "neoforge" if instance.loader == Loader::NeoForge => {
                    Some(instance.loader_version.as_deref())
                }
                _ => None,
            };
            let present = builtin.is_some() || !active.is_empty();
            let matched = if let Some(version) = builtin {
                matches(version, &dep.ranges, &dep.dialect)
            } else if active.len() > 1 {
                None
            } else {
                active
                    .first()
                    .and_then(|(_, v)| matches(v.version.as_deref(), &dep.ranges, &dep.dialect))
            };
            let status = if dep.relation == "optional" && (dep.dialect == "fabric" || !present) {
                "optional"
            } else if !matches!(
                dep.relation.as_str(),
                "required" | "optional" | "recommended" | "breaks" | "conflict"
            ) {
                "unknown"
            } else if dep.relation == "breaks" || dep.relation == "conflict" {
                if !present {
                    if complete { "clear" } else { "unknown" }
                } else {
                    match matched {
                        Some(true) => "conflict",
                        Some(false) => "clear",
                        None => "unknown",
                    }
                }
            } else if !present {
                if !complete {
                    "unknown"
                } else if candidates.iter().any(|(i, _)| !nodes[*i].row.enabled) {
                    "disabled"
                } else {
                    "missing"
                }
            } else {
                match matched {
                    Some(true) => "satisfied",
                    Some(false) => "version_mismatch",
                    None => "unknown",
                }
            };
            let severity = if dep.dialect == "maven"
                && dep.relation == "optional"
                && status == "version_mismatch"
            {
                "error"
            } else {
                match (dep.relation.as_str(), status) {
                    ("required", "missing" | "disabled" | "version_mismatch")
                    | ("breaks", "conflict") => "error",
                    (_, "unknown")
                    | ("conflict", "conflict")
                    | ("recommended", "missing" | "disabled" | "version_mismatch")
                    | ("optional", "version_mismatch") => "warning",
                    _ => "info",
                }
            };
            let targets = if let Some(version) = builtin {
                vec![DependencyTarget {
                    title: dep.id.clone(),
                    version: version.map(String::from),
                    directory: String::new(),
                    filename: String::new(),
                    enabled: true,
                }]
            } else {
                candidates
                    .iter()
                    .take(8)
                    .map(|(i, v)| DependencyTarget {
                        title: nodes[*i].row.title.clone(),
                        version: v.version.clone(),
                        directory: nodes[*i].row.directory.clone(),
                        filename: nodes[*i].row.filename.clone(),
                        enabled: nodes[*i].row.enabled,
                    })
                    .collect()
            };
            checks[i].push(DependencyCheck {
                dependency: dep.clone(),
                status: status.into(),
                severity: severity.into(),
                targets,
            });
        }
    }
    let mut errors = 0;
    let mut warnings = 0;
    for (i, node) in nodes.iter_mut().enumerate() {
        node.row.checks = std::mem::take(&mut checks[i]);
        if duplicate[i] {
            node.row.warnings.push("duplicate_mod_id".into());
        }
        if node.row.enabled {
            errors += u32::from(matches!(
                node.row.status.as_str(),
                "incompatible" | "unreadable"
            ));
            warnings += node.row.warnings.len() as u32;
            errors += node
                .row
                .checks
                .iter()
                .filter(|c| c.severity == "error")
                .count() as u32;
            warnings += node
                .row
                .checks
                .iter()
                .filter(|c| c.severity == "warning")
                .count() as u32;
        }
    }
    let report = ModDiagnostics {
        complete,
        errors,
        warnings,
        mods: nodes.into_iter().map(|n| n.row).collect(),
    };
    if serde_json::to_vec(&report)?.len() > 12_000_000 {
        return Err(CoreError::Integrity);
    }
    Ok(report)
}
pub(super) fn selected_issues(report: &ModDiagnostics, files: &[ContentRecord]) -> bool {
    report
        .mods
        .iter()
        .filter(|m| {
            files.iter().any(|f| {
                f.directory == m.directory && f.file.filename.eq_ignore_ascii_case(&m.filename)
            })
        })
        .any(|m| m.checks.iter().any(|c| c.severity != "info"))
}
