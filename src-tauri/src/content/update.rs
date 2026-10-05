use super::{install, manage, model::*, provider::ContentProvider, resolve};
use crate::{
    error::CoreError,
    game::fs::{read_limited, write_atomic},
    instances::{
        filesystem::Paths,
        model::{Instance, now},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policies {
    schema: u32,
    items: Vec<ContentUpdatePolicy>,
}
pub(super) fn policies(
    paths: &Paths,
    directory: &Path,
) -> Result<Vec<ContentUpdatePolicy>, CoreError> {
    let file = paths.checked(&directory.join(".sporium/content-update-policy.json"))?;
    if !file.exists() {
        return Ok(vec![]);
    }
    let value: Policies = serde_json::from_slice(&read_limited(paths, &file, 1_000_000)?)?;
    if value.schema != 1 {
        return Err(CoreError::SchemaTooNew);
    }
    if value.items.len() > 4096 {
        return Err(CoreError::Integrity);
    }
    let mut seen = HashSet::new();
    for item in &value.items {
        super::modrinth::valid_id(&item.project_id)?;
        if let Some(id) = &item.ignored_version {
            super::modrinth::valid_id(id)?;
        }
        if !seen.insert(&item.project_id) {
            return Err(CoreError::Integrity);
        }
    }
    Ok(value.items)
}
pub(super) fn set_policy(
    paths: &Paths,
    directory: &Path,
    policy: ContentUpdatePolicy,
) -> Result<(), CoreError> {
    super::modrinth::valid_id(&policy.project_id)?;
    if let Some(id) = &policy.ignored_version {
        super::modrinth::valid_id(id)?;
    }
    if !install::records(paths, directory)?
        .iter()
        .any(|r| r.provider == "modrinth" && r.project_id == policy.project_id)
    {
        return Err(CoreError::NotFound);
    }
    let mut items = policies(paths, directory)?;
    items.retain(|p| p.project_id != policy.project_id);
    if policy.pinned || policy.ignored_version.is_some() {
        items.push(policy);
    }
    write_atomic(
        paths,
        &directory.join(".sporium/content-update-policy.json"),
        &serde_json::to_vec(&Policies { schema: 1, items })?,
    )
}
pub(super) fn check(
    provider: &dyn ContentProvider,
    instance: &Instance,
    installed: &[ContentRecord],
    policies: &[ContentUpdatePolicy],
) -> Vec<ContentUpdate> {
    let mut seen = HashSet::new();
    installed
        .iter()
        .filter(|r| seen.insert(r.project_id.clone()))
        .map(|record| {
            let policy = policies
                .iter()
                .find(|p| p.project_id == record.project_id)
                .cloned()
                .unwrap_or(ContentUpdatePolicy {
                    project_id: record.project_id.clone(),
                    ..Default::default()
                });
            let mut item = ContentUpdate {
                project_id: record.project_id.clone(),
                title: record.title.clone(),
                current_version: record.version.id.clone(),
                status: "local".into(),
                candidate: None,
                policy,
            };
            if record.provider != "modrinth" {
                return item;
            }
            let result = (|| {
                let project = provider.project(&record.project_id)?;
                if project.id != record.project_id {
                    return Err(CoreError::Integrity);
                }
                let versions = provider.versions(&record.project_id, "")?;
                let newer = |v: &&ContentVersion| {
                    v.project_id == project.id
                        && v.id != record.version.id
                        && v.date_published > record.version.date_published
                        && v.status == "listed"
                        && (v.version_type == "release"
                            || (record.version.version_type == "beta" && v.version_type == "beta")
                            || record.version.version_type == "alpha")
                };
                let incompatible = versions
                    .iter()
                    .filter(newer)
                    .any(|v| !resolve::compatible(&project, v, instance));
                let mut candidates: Vec<_> = versions
                    .iter()
                    .filter(newer)
                    .filter(|v| {
                        resolve::compatible(&project, v, instance)
                            && (resolve::selected_files(&project, v, false).is_ok()
                                || (super::worlds::is_datapack(&project, v)
                                    && super::worlds::selected(&project, v, "validation", false)
                                        .is_ok()))
                    })
                    .collect();
                candidates.sort_by(|a, b| {
                    b.date_published
                        .cmp(&a.date_published)
                        .then_with(|| a.id.cmp(&b.id))
                });
                item.candidate = candidates.first().map(|v| (*v).clone());
                item.status = if item.policy.pinned {
                    "pinned"
                } else if item
                    .candidate
                    .as_ref()
                    .is_some_and(|v| item.policy.ignored_version.as_ref() == Some(&v.id))
                {
                    "ignored"
                } else if item.candidate.is_some() {
                    "available"
                } else if incompatible {
                    "incompatible"
                } else {
                    "current"
                }
                .into();
                Ok::<_, CoreError>(())
            })();
            if result.is_err() {
                item.status = "unavailable".into();
            }
            item
        })
        .collect()
}
pub(super) fn plan(
    provider: &dyn ContentProvider,
    instance: &Instance,
    installed: &[ContentRecord],
    policies: &[ContentUpdatePolicy],
    selections: &[ContentSelection],
) -> Result<ContentUpdatePlan, CoreError> {
    if selections.is_empty() || selections.len() > 4096 {
        return Err(CoreError::InvalidInput);
    }
    let mut projects = HashSet::new();
    let mut chosen = vec![];
    for item in selections {
        let record = installed
            .iter()
            .find(|r| {
                r.directory == item.directory
                    && r.file.filename == item.filename
                    && r.file.hashes.sha512 == item.sha512
            })
            .ok_or(CoreError::RecordConflict)?;
        if record.provider != "modrinth" {
            return Err(CoreError::ContentUnsupported);
        }
        if projects.insert(record.project_id.clone()) {
            chosen.push(record.clone());
        }
    }
    let checked = check(provider, instance, &chosen, policies);
    let requests: Vec<_> = checked
        .into_iter()
        .map(|u| {
            if u.status != "available" {
                return Err(CoreError::ContentConflict);
            }
            Ok(ContentRequest {
                instance_id: instance.id.clone(),
                project_id: u.project_id,
                version_id: u.candidate.ok_or(CoreError::NotFound)?.id,
            })
        })
        .collect::<Result<_, CoreError>>()?;
    let mut plan = resolve::resolve_many(provider, &requests, instance, installed, true)?;
    for record in &mut plan.files {
        let old: Vec<_> = installed
            .iter()
            .filter(|r| r.project_id == record.project_id)
            .collect();
        if old.iter().any(|r| r.provider != "modrinth") {
            return Err(CoreError::ContentConflict);
        }
        let changed = old.iter().any(|r| r.version.id != record.version.id);
        if changed
            && policies.iter().any(|p| {
                p.project_id == record.project_id
                    && (p.pinned || p.ignored_version.as_ref() == Some(&record.version.id))
            })
        {
            return Err(CoreError::DependencyConflict);
        }
        if record.directory == "mods" && old.iter().any(|r| r.directory == "mods_disabled") {
            if record.dependency {
                return Err(CoreError::DependencyConflict);
            }
            record.directory = "mods_disabled".into();
        }
        if old.iter().any(|r| !r.dependency) {
            record.dependency = false;
        }
    }
    let replaced: HashSet<_> = plan.files.iter().map(|r| r.project_id.clone()).collect();
    let previous: Vec<_> = installed
        .iter()
        .filter(|r| replaced.contains(&r.project_id))
        .cloned()
        .collect();
    // Keep unchanged dependencies out of the download/commit set.
    let unchanged: HashSet<_> = replaced
        .iter()
        .filter(|id| {
            let old: Vec<_> = previous.iter().filter(|r| &r.project_id == *id).collect();
            let new: Vec<_> = plan.files.iter().filter(|r| &r.project_id == *id).collect();
            old.len() == new.len()
                && new.iter().all(|n| {
                    old.iter().any(|o| {
                        o.directory == n.directory
                            && o.file.filename == n.file.filename
                            && o.version.id == n.version.id
                            && o.file.hashes.sha512 == n.file.hashes.sha512
                    })
                })
        })
        .cloned()
        .collect();
    plan.files.retain(|r| !unchanged.contains(&r.project_id));
    plan.total_bytes = plan.files.iter().map(|r| r.file.size).sum();
    let previous = previous
        .into_iter()
        .filter(|r| !unchanged.contains(&r.project_id))
        .collect();
    install::validate_records(&plan.files)?;
    Ok(ContentUpdatePlan { plan, previous })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateJournal {
    schema: u32,
    token: String,
    before: Vec<ContentRecord>,
    after: Vec<ContentRecord>,
    previous: Vec<ContentRecord>,
    files: Vec<ContentRecord>,
    event: ContentHistory,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    pub schema: u32,
    pub instance: Instance,
    pub files: Vec<ContentRecord>,
    pub settings: Vec<String>,
    #[serde(default)]
    pub settings_hashes: std::collections::BTreeMap<String, crate::projects::transaction::Blob>,
}
pub(super) fn snapshot_file(
    paths: &Paths,
    directory: &Path,
    token: &str,
    index: usize,
) -> Result<PathBuf, CoreError> {
    crate::instances::model::valid_id(token)?;
    paths.checked(
        &directory
            .join(".sporium/content-snapshots")
            .join(token)
            .join("files")
            .join(format!("{index}.bin")),
    )
}
fn same(a: &[ContentRecord], b: &[ContentRecord]) -> Result<bool, CoreError> {
    Ok(serde_json::to_vec(a)? == serde_json::to_vec(b)?)
}
fn after(
    before: &[ContentRecord],
    previous: &[ContentRecord],
    files: &[ContentRecord],
) -> Result<Vec<ContentRecord>, CoreError> {
    let ids: HashSet<_> = files.iter().map(|r| &r.project_id).collect();
    let expected: Vec<_> = before
        .iter()
        .filter(|r| ids.contains(&r.project_id))
        .cloned()
        .collect();
    if !same(&expected, previous)?
        || previous.iter().any(|r| r.provider != "modrinth")
        || files.iter().any(|r| r.provider != "modrinth")
    {
        return Err(CoreError::Integrity);
    }
    let mut result: Vec<_> = before
        .iter()
        .filter(|r| !ids.contains(&r.project_id))
        .cloned()
        .collect();
    result.extend_from_slice(files);
    install::validate_records(&result)?;
    Ok(result)
}
pub(super) fn preflight(
    paths: &Paths,
    directory: &Path,
    before: &[ContentRecord],
    previous: &[ContentRecord],
    files: &[ContentRecord],
) -> Result<(), CoreError> {
    for old in previous {
        crate::projects::guard_user_file(
            paths,
            directory,
            &format!("{}/{}", old.directory, old.file.filename),
            false,
        )?;
    }
    for new in files {
        crate::projects::guard_user_file(
            paths,
            directory,
            &format!("{}/{}", new.directory, new.file.filename),
            true,
        )?;
    }
    after(before, previous, files)?;
    for old in before {
        if !install::verified(&install::target(paths, directory, old)?, &old.file)? {
            return Err(CoreError::ContentConflict);
        }
    }
    for new in files {
        let target = install::target(paths, directory, new)?;
        if target.exists()
            && !previous.iter().any(|o| {
                o.directory == new.directory
                    && o.file.filename.eq_ignore_ascii_case(&new.file.filename)
            })
        {
            return Err(CoreError::ContentConflict);
        }
        if matches!(new.directory.as_str(), "mods" | "mods_disabled") {
            let other = paths.checked(
                &directory
                    .join(if new.directory == "mods" {
                        "mods_disabled"
                    } else {
                        "mods"
                    })
                    .join(&new.file.filename),
            )?;
            if other.exists() {
                return Err(CoreError::ContentConflict);
            }
        }
    }
    Ok(())
}
pub(super) fn copy_verified(
    paths: &Paths,
    source: &Path,
    target: &Path,
    file: &ContentFile,
) -> Result<(), CoreError> {
    paths.checked(source)?;
    paths.checked(target)?;
    if !install::verified(source, file)? {
        return Err(CoreError::ContentConflict);
    }
    paths.mkdir(target.parent().ok_or(CoreError::UnsafePath)?)?;
    let mut temp = tempfile::NamedTempFile::new_in(target.parent().ok_or(CoreError::UnsafePath)?)?;
    std::io::copy(&mut fs::File::open(source)?, &mut temp)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    if !install::verified(temp.path(), file)? {
        return Err(CoreError::Integrity);
    }
    paths.checked(target)?;
    temp.persist_noclobber(target)
        .map_err(|e| CoreError::Io(e.error))?;
    Ok(())
}
fn settings_files(paths: &Paths, directory: &Path) -> Result<Vec<PathBuf>, CoreError> {
    let mut pending = vec![directory.join("config")];
    let mut files = vec![];
    for name in [
        "options.txt",
        "optionsof.txt",
        "optionsshaders.txt",
        "servers.dat",
    ] {
        let file = paths.checked(&directory.join(name))?;
        if file.exists() {
            files.push(file);
        }
    }
    let mut bytes = 0u64;
    while let Some(path) = pending.pop() {
        let path = paths.checked(&path)?;
        if !path.exists() {
            continue;
        }
        if path.is_dir() {
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
        } else {
            files.push(path);
        }
        if pending.len() + files.len() > 4096 {
            return Err(CoreError::InvalidInput);
        }
    }
    for file in &files {
        bytes = bytes
            .checked_add(file.metadata()?.len())
            .ok_or(CoreError::InvalidInput)?;
        if bytes > 128_000_000 || file.metadata()?.len() > 32_000_000 {
            return Err(CoreError::InvalidInput);
        }
    }
    Ok(files)
}
pub(super) fn commit(
    paths: &Paths,
    directory: &Path,
    instance: &Instance,
    plan: &ContentPlan,
    previous: &[ContentRecord],
) -> Result<(), CoreError> {
    let before = install::records(paths, directory)?;
    preflight(paths, directory, &before, previous, &plan.files)?;
    let next = after(&before, previous, &plan.files)?;
    for (index, record) in plan.files.iter().enumerate() {
        if !install::verified(
            &install::stage(paths, directory, &plan.token, index)?,
            &record.file,
        )? {
            return Err(CoreError::Integrity);
        }
    }
    let settings = settings_files(paths, directory)?;
    let root = paths.checked(
        &directory
            .join(".sporium/content-snapshots")
            .join(&plan.token),
    )?;
    for (index, record) in before.iter().enumerate() {
        copy_verified(
            paths,
            &install::target(paths, directory, record)?,
            &snapshot_file(paths, directory, &plan.token, index)?,
            &record.file,
        )?;
    }
    let mut names = vec![];
    let mut settings_hashes = std::collections::BTreeMap::new();
    for file in settings {
        let relative = file
            .strip_prefix(directory)
            .map_err(|_| CoreError::UnsafePath)?;
        write_atomic(
            paths,
            &root.join("settings").join(relative),
            &read_limited(paths, &file, 32_000_000)?,
        )?;
        let name = relative.to_string_lossy().replace('\\', "/");
        let blob = crate::projects::transaction::digest(&root.join("settings").join(relative))?;
        settings_hashes.insert(name.clone(), blob);
        names.push(name);
    }
    write_atomic(
        paths,
        &root.join("snapshot.json"),
        &serde_json::to_vec(&Snapshot {
            schema: 1,
            instance: instance.clone(),
            files: before.clone(),
            settings: names,
            settings_hashes,
        })?,
    )?;
    manage::history(paths, directory)?;
    preflight(paths, directory, &before, previous, &plan.files)?;
    let event = ContentHistory {
        id: plan.token.clone(),
        timestamp: now(),
        action: "update".into(),
        titles: plan
            .files
            .iter()
            .map(|n| {
                previous
                    .iter()
                    .find(|o| o.project_id == n.project_id)
                    .map(|o| {
                        format!(
                            "{}: {} → {}",
                            n.title, o.version.version_number, n.version.version_number
                        )
                    })
                    .unwrap_or_else(|| format!("{}: +{}", n.title, n.version.version_number))
            })
            .collect(),
    };
    let journal = UpdateJournal {
        schema: 1,
        token: plan.token.clone(),
        before,
        after: next,
        previous: previous.to_vec(),
        files: plan.files.clone(),
        event,
    };
    let path = paths.checked(&directory.join(".sporium/content-update.json"))?;
    if path.exists() {
        return Err(CoreError::ContentConflict);
    }
    write_atomic(paths, &path, &serde_json::to_vec(&journal)?)?;
    recover(paths, directory)
}
pub(super) fn recover(paths: &Paths, directory: &Path) -> Result<(), CoreError> {
    let path = paths.checked(&directory.join(".sporium/content-update.json"))?;
    if !path.exists() {
        return Ok(());
    }
    let journal: UpdateJournal = serde_json::from_slice(&read_limited(paths, &path, 64_000_000)?)?;
    if journal.schema != 1 {
        return Err(CoreError::SchemaTooNew);
    }
    crate::instances::model::valid_id(&journal.token)?;
    for list in [
        &journal.before,
        &journal.after,
        &journal.previous,
        &journal.files,
    ] {
        install::validate_records(list)?;
    }
    if !same(
        &after(&journal.before, &journal.previous, &journal.files)?,
        &journal.after,
    )? || journal.event.id != journal.token
        || journal.event.action != "update"
    {
        return Err(CoreError::Integrity);
    }
    let current = install::records(paths, directory)?;
    if !same(&current, &journal.before)? && !same(&current, &journal.after)? {
        return Err(CoreError::RecordConflict);
    }
    let snapshot: Snapshot = serde_json::from_slice(&read_limited(
        paths,
        &directory
            .join(".sporium/content-snapshots")
            .join(&journal.token)
            .join("snapshot.json"),
        32_000_000,
    )?)?;
    if snapshot.schema != 1
        || !same(&snapshot.files, &journal.before)?
        || directory.file_name().and_then(|v| v.to_str()) != Some(snapshot.instance.id.as_str())
    {
        return Err(CoreError::Integrity);
    }
    for (index, old) in journal.before.iter().enumerate() {
        if !install::verified(
            &snapshot_file(paths, directory, &journal.token, index)?,
            &old.file,
        )? {
            return Err(CoreError::Integrity);
        }
    }
    for (index, new) in journal.files.iter().enumerate() {
        if !install::verified(
            &install::stage(paths, directory, &journal.token, index)?,
            &new.file,
        )? {
            return Err(CoreError::Integrity);
        }
    }
    // Validate the whole transaction before removing or publishing any file.
    for old in &journal.previous {
        let target = install::target(paths, directory, old)?;
        if target.exists()
            && !install::verified(&target, &old.file)?
            && !journal.files.iter().any(|n| {
                n.directory == old.directory
                    && n.file.filename.eq_ignore_ascii_case(&old.file.filename)
                    && install::verified(&target, &n.file).unwrap_or(false)
            })
        {
            return Err(CoreError::ContentConflict);
        }
    }
    for new in &journal.files {
        let target = install::target(paths, directory, new)?;
        if target.exists()
            && !install::verified(&target, &new.file)?
            && !journal.previous.iter().any(|o| {
                o.directory == new.directory
                    && o.file.filename.eq_ignore_ascii_case(&new.file.filename)
                    && install::verified(&target, &o.file).unwrap_or(false)
            })
        {
            return Err(CoreError::ContentConflict);
        }
        if matches!(new.directory.as_str(), "mods" | "mods_disabled")
            && paths
                .checked(
                    &directory
                        .join(if new.directory == "mods" {
                            "mods_disabled"
                        } else {
                            "mods"
                        })
                        .join(&new.file.filename),
                )?
                .exists()
        {
            return Err(CoreError::ContentConflict);
        }
    }
    for old in &journal.previous {
        let target = install::target(paths, directory, old)?;
        if target.exists()
            && !journal.files.iter().any(|n| {
                n.directory == old.directory
                    && n.file.filename.eq_ignore_ascii_case(&old.file.filename)
                    && install::verified(&target, &n.file).unwrap_or(false)
            })
        {
            if !install::verified(&target, &old.file)? {
                return Err(CoreError::ContentConflict);
            }
            paths.remove(&target)?;
        }
    }
    for (index, new) in journal.files.iter().enumerate() {
        let target = install::target(paths, directory, new)?;
        if !target.exists() {
            copy_verified(
                paths,
                &install::stage(paths, directory, &journal.token, index)?,
                &target,
                &new.file,
            )?;
        }
    }
    install::write_records(paths, directory, &journal.after)?;
    manage::append_history(
        paths,
        directory,
        ContentHistory {
            id: format!("snapshot:{}", journal.token),
            timestamp: journal.event.timestamp,
            action: "snapshot".into(),
            titles: vec![snapshot.instance.name],
        },
    )?;
    manage::append_history(paths, directory, journal.event)?;
    paths.remove(&path)?;
    paths.remove(
        &directory
            .join(".sporium/content-staging")
            .join(&journal.token),
    )?;
    Ok(())
}
