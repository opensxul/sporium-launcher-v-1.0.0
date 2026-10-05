//! Dependency identity comes only from verified official file hashes, never mod names.
use super::{diagnostics, install, local, model::*, provider::ContentProvider, resolve};
use crate::{
    error::CoreError,
    instances::{filesystem::Paths, model::Instance},
};
use std::{collections::HashSet, path::Path};
pub(super) type Proof = (Vec<ContentSelection>, Vec<u8>, Vec<u8>);

pub(super) fn matched(
    provider: &dyn ContentProvider,
    instance: &Instance,
    original: &ContentRecord,
) -> Result<Option<ContentRecord>, CoreError> {
    let Some(version) = provider.version_from_hash(&original.file.hashes.sha512)? else {
        return Ok(None);
    };
    super::modrinth::valid_id(&version.id)?;
    super::modrinth::valid_id(&version.project_id)?;
    let file = version
        .files
        .iter()
        .find(|f| {
            f.size == original.file.size
                && f.hashes
                    .sha1
                    .eq_ignore_ascii_case(&original.file.hashes.sha1)
                && f.hashes
                    .sha512
                    .eq_ignore_ascii_case(&original.file.hashes.sha512)
        })
        .ok_or(CoreError::Integrity)?;
    resolve::validate_file(file, "mods")?;
    if matches!(
        file.file_type.as_deref(),
        Some("sources-jar" | "dev-jar" | "javadoc-jar" | "signature")
    ) {
        return Err(CoreError::ContentUnsupported);
    }
    let project = provider.project(&version.project_id)?;
    if project.id != version.project_id
        || project.project_type != "mod"
        || super::worlds::is_datapack(&project, &version)
        || !resolve::compatible(&project, &version, instance)
    {
        return Err(CoreError::ContentIncompatible);
    }
    let mut record = original.clone();
    record.file = file.clone();
    record.file.filename.clone_from(&original.file.filename);
    record.project_id = project.id;
    record.title = project.title;
    record.icon_url = project.icon_url;
    record.version = version;
    record.provider = "modrinth".into();
    record.local = None;
    Ok(Some(record))
}
pub(super) fn resolve(
    provider: &dyn ContentProvider,
    instance: &Instance,
    installed: &[ContentRecord],
    parents: &[ContentRecord],
) -> Result<ContentPlan, CoreError> {
    if parents.is_empty() || parents.len() > 16 {
        return Err(CoreError::ContentUnsupported);
    }
    let roots: HashSet<_> = parents.iter().map(|r| r.project_id.clone()).collect();
    let mut virtual_installed: Vec<_> = installed
        .iter()
        .filter(|r| !roots.contains(&r.project_id))
        .cloned()
        .collect();
    virtual_installed.extend_from_slice(parents);
    let requests: Vec<_> = parents
        .iter()
        .map(|r| ContentRequest {
            instance_id: instance.id.clone(),
            project_id: r.project_id.clone(),
            version_id: r.version.id.clone(),
        })
        .collect();
    let mut plan = resolve::resolve_many(provider, &requests, instance, &virtual_installed, false)?;
    plan.files.retain(|r| !roots.contains(&r.project_id));
    plan.total_bytes = plan.files.iter().map(|r| r.file.size).sum();
    Ok(plan)
}
pub(super) fn staged_diagnostics(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
    plan: &ContentPlan,
) -> Result<ModDiagnostics, CoreError> {
    let mut additions = plan.files.clone();
    for (index, record) in additions.iter_mut().enumerate() {
        if record.kind == "mod" {
            let staged = install::stage(paths, directory, &plan.token, index)?;
            if !install::verified(&staged, &record.file)? {
                return Err(CoreError::Integrity);
            }
            record.local = Some(local::inspect(&staged, instance)?.2);
        }
    }
    diagnostics::scan(paths, directory, instance, &additions)
}
pub(super) fn block_known_errors(
    report: &ModDiagnostics,
    selected: &[ContentSelection],
) -> Result<(), CoreError> {
    if report
        .mods
        .iter()
        .filter(|m| {
            m.enabled
                && selected.iter().any(|s| {
                    s.directory == m.directory && s.filename.eq_ignore_ascii_case(&m.filename)
                })
        })
        .any(|m| {
            m.status != "readable" && m.status != "unknown"
                || m.warnings.iter().any(|w| w == "duplicate_mod_id")
                || m.checks.iter().any(|c| c.severity == "error")
        })
    {
        return Err(CoreError::DependencyConflict);
    }
    Ok(())
}
pub(super) fn selections(records: &[ContentRecord]) -> Vec<ContentSelection> {
    records
        .iter()
        .map(|r| ContentSelection {
            directory: r.directory.clone(),
            filename: r.file.filename.clone(),
            sha512: r.file.hashes.sha512.clone(),
        })
        .collect()
}
pub(super) fn verify_context(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
    proof: &(Vec<ContentSelection>, Vec<u8>, Vec<u8>),
) -> Result<(), CoreError> {
    if serde_json::to_vec(instance)? != proof.2
        || serde_json::to_vec(&diagnostics::scan(paths, directory, instance, &[])?)? != proof.1
    {
        return Err(CoreError::RecordConflict);
    }
    let installed = install::records(paths, directory)?;
    for source in &proof.0 {
        let record = installed
            .iter()
            .find(|r| {
                r.directory == source.directory
                    && r.file.filename == source.filename
                    && r.file.hashes.sha512 == source.sha512
            })
            .ok_or(CoreError::RecordConflict)?;
        if !install::verified(&install::target(paths, directory, record)?, &record.file)? {
            return Err(CoreError::SourceChanged);
        }
    }
    Ok(())
}
