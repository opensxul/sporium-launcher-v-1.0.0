use super::{model::*, modrinth::valid_id, provider::ContentProvider};
use crate::{
    error::CoreError,
    instances::model::{Instance, Loader},
};
use std::collections::{HashMap, VecDeque};

pub fn loader_name(loader: Loader) -> &'static str {
    match loader {
        Loader::Vanilla => "minecraft",
        Loader::Fabric => "fabric",
        Loader::Forge => "forge",
        Loader::NeoForge => "neoforge",
    }
}
pub fn installable(kind: &str) -> bool {
    matches!(kind, "mod" | "resourcepack" | "shader" | "datapack")
}
pub fn plugin_compatible(
    project: &ContentProject,
    version: &ContentVersion,
    minecraft: &str,
    platform: &str,
) -> bool {
    // V2 may expose a multi-platform plugin project as `mod`. The concrete version's
    // declared platform, environment and JAR are authoritative, never its title.
    matches!(platform, "paper" | "purpur")
        && matches!(project.project_type.as_str(), "mod" | "plugin")
        && version.project_id == project.id
        && matches!(version.status.as_str(), "listed" | "archived" | "unlisted")
        && version.game_versions.iter().any(|v| v == minecraft)
        && version.loaders.iter().any(|v| v == platform)
        && match version.environment.as_str() {
            "server_only"
            | "dedicated_server_only"
            | "server_only_client_optional"
            | "client_and_server"
            | "client_or_server"
            | "client_or_server_prefers_both" => true,
            "" | "unknown" => matches!(project.server_side.as_str(), "required" | "optional"),
            _ => false,
        }
        && version
            .files
            .iter()
            .find(|f| f.primary)
            .or_else(|| version.files.first())
            .is_some_and(|f| {
                !matches!(
                    f.file_type.as_deref(),
                    Some("sources-jar" | "dev-jar" | "javadoc-jar" | "signature")
                ) && validate_file(f, "mods").is_ok()
            })
}
pub fn compatible(project: &ContentProject, version: &ContentVersion, instance: &Instance) -> bool {
    if super::worlds::is_datapack(project, version) {
        return super::worlds::compatible(project, version, instance);
    }
    if version.project_id != project.id
        || !matches!(version.status.as_str(), "listed" | "archived" | "unlisted")
        || !version.game_versions.contains(&instance.minecraft_version)
        || !installable(&project.project_type)
    {
        return false;
    }
    let environment = version.environment.as_str();
    if environment == "dedicated_server_only" {
        return false;
    }
    if (environment.is_empty() || environment == "unknown") && project.client_side == "unsupported"
    {
        return false;
    }
    let loaders = &version.loaders;
    match project.project_type.as_str() {
        "mod" => {
            instance.loader != Loader::Vanilla
                && loaders.iter().any(|s| s == loader_name(instance.loader))
        }
        "resourcepack" => loaders.iter().any(|s| s == "minecraft"),
        "shader" => loaders
            .iter()
            .any(|s| matches!(s.as_str(), "iris" | "optifine")),
        _ => false,
    }
}
pub fn validate_file(file: &ContentFile, directory: &str) -> Result<(), CoreError> {
    validate_payload(file, directory)?;
    let url = reqwest::Url::parse(&file.url).map_err(|_| CoreError::Integrity)?;
    if !crate::game::network::approved(&url) || url.host_str() != Some("cdn.modrinth.com") {
        return Err(CoreError::Integrity);
    }
    Ok(())
}
pub fn validate_payload(file: &ContentFile, directory: &str) -> Result<(), CoreError> {
    let name = crate::game::fs::relative(&file.filename)?;
    if name.components().count() != 1
        || file.filename.len() > 180
        || file.size == 0
        || file.size > 1_000_000_000
    {
        return Err(CoreError::InvalidInput);
    }
    let extension = name.extension().and_then(|s| s.to_str()).unwrap_or("");
    if !match directory {
        "mods" | "mods_disabled" => extension.eq_ignore_ascii_case("jar"),
        "resourcepacks" | "shaderpacks" => extension.eq_ignore_ascii_case("zip"),
        _ => super::worlds::datapack_directory(directory) && extension.eq_ignore_ascii_case("zip"),
    } {
        return Err(CoreError::ContentUnsupported);
    }
    for (hash, length) in [(&file.hashes.sha1, 40), (&file.hashes.sha512, 128)] {
        if hash.len() != length || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(CoreError::Integrity);
        }
    }
    Ok(())
}
pub fn selected_files(
    project: &ContentProject,
    version: &ContentVersion,
    dependency: bool,
) -> Result<Vec<ContentRecord>, CoreError> {
    let primary = version
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| version.files.first())
        .ok_or(CoreError::Integrity)?;
    let directory = match project.project_type.as_str() {
        "mod" => "mods",
        "resourcepack" => "resourcepacks",
        "shader" => "shaderpacks",
        _ => return Err(CoreError::ContentUnsupported),
    };
    let mut files = vec![];
    for file in &version.files {
        let required_pack = file.file_type.as_deref() == Some("required-resource-pack");
        if file.filename != primary.filename && !required_pack {
            continue;
        }
        if matches!(
            file.file_type.as_deref(),
            Some("sources-jar" | "dev-jar" | "javadoc-jar" | "signature")
        ) {
            return Err(CoreError::ContentUnsupported);
        }
        let directory = if required_pack {
            "resourcepacks"
        } else {
            directory
        };
        validate_file(file, directory)?;
        files.push(ContentRecord {
            local: None,
            icon_url: project.icon_url.clone(),
            provider: "modrinth".into(),
            project_id: project.id.clone(),
            title: project.title.clone(),
            kind: project.project_type.clone(),
            version: version.clone(),
            file: file.clone(),
            directory: directory.into(),
            dependency,
        });
    }
    Ok(files)
}
pub fn resolve(
    provider: &dyn ContentProvider,
    request: &ContentRequest,
    instance: &Instance,
    installed: &[ContentRecord],
) -> Result<ContentPlan, CoreError> {
    resolve_many(
        provider,
        std::slice::from_ref(request),
        instance,
        installed,
        false,
    )
}
pub(super) fn resolve_many(
    provider: &dyn ContentProvider,
    requests: &[ContentRequest],
    instance: &Instance,
    installed: &[ContentRecord],
    updating: bool,
) -> Result<ContentPlan, CoreError> {
    let mut pending = VecDeque::new();
    for request in requests {
        valid_id(&request.project_id)?;
        valid_id(&request.version_id)?;
        pending.push_back((
            Some(request.project_id.clone()),
            Some(request.version_id.clone()),
            false,
        ));
    }
    let mut selected: HashMap<String, ContentVersion> = HashMap::new();
    let mut files = vec![];
    let mut optional = 0;
    let mut visited = 0;
    while let Some((project_id, version_id, dependency)) = pending.pop_front() {
        visited += 1;
        if visited > (if updating { 16384 } else { 256 })
            || selected.len() >= (if updating { 4096 } else { 64 })
        {
            return Err(CoreError::DependencyConflict);
        }
        let pinned = version_id
            .as_deref()
            .map(|id| provider.version(id))
            .transpose()?;
        if let (Some(id), Some(version)) = (&project_id, &pinned)
            && id != &version.project_id
        {
            return Err(CoreError::Integrity);
        }
        let id = project_id
            .or_else(|| pinned.as_ref().map(|v| v.project_id.clone()))
            .ok_or(CoreError::DependencyConflict)?;
        if let Some(existing) = selected.get(&id) {
            if pinned.is_some_and(|v| v.id != existing.id) {
                return Err(CoreError::DependencyConflict);
            }
            continue;
        }
        let project = provider.project(&id)?;
        if project.id != id {
            return Err(CoreError::Integrity);
        }
        let version = if let Some(pinned) = pinned {
            pinned
        } else if let Some(record) = installed.iter().find(|r| r.project_id == id) {
            provider.version(&record.version.id)?
        } else {
            let mut versions = provider.versions(&id, &instance.minecraft_version)?;
            versions.retain(|v| compatible(&project, v, instance));
            versions.sort_by(|a, b| {
                (a.version_type != "release")
                    .cmp(&(b.version_type != "release"))
                    .then_with(|| b.date_published.cmp(&a.date_published))
            });
            versions
                .into_iter()
                .next()
                .ok_or(CoreError::DependencyConflict)?
        };
        if !compatible(&project, &version, instance) {
            return Err(CoreError::ContentIncompatible);
        }
        if !updating
            && installed
                .iter()
                .any(|r| r.project_id == id && r.version.id != version.id)
        {
            return Err(CoreError::ContentConflict);
        }
        for dep in &version.dependencies {
            match dep.dependency_type.as_str() {
                "required" => {
                    if dep.project_id.is_none() && dep.version_id.is_none() {
                        return Err(CoreError::DependencyConflict);
                    }
                    pending.push_back((dep.project_id.clone(), dep.version_id.clone(), true));
                }
                "optional" => optional += 1,
                "embedded" | "incompatible" => (),
                _ => return Err(CoreError::DependencyConflict),
            }
        }
        if super::worlds::is_datapack(&project, &version) && updating {
            let mut world_ids: std::collections::HashSet<String> = installed
                .iter()
                .filter(|r| r.project_id == id && super::worlds::datapack_directory(&r.directory))
                .filter_map(|r| r.directory.split('/').nth(1).map(String::from))
                .collect();
            for parent in &files {
                let parent: &ContentRecord = parent;
                if super::worlds::datapack_directory(&parent.directory)
                    && parent.version.dependencies.iter().any(|d| {
                        d.dependency_type == "required"
                            && (d.project_id.as_ref() == Some(&id)
                                || d.version_id.as_ref() == Some(&version.id))
                    })
                {
                    world_ids.insert(
                        parent
                            .directory
                            .split('/')
                            .nth(1)
                            .ok_or(CoreError::UnsafePath)?
                            .into(),
                    );
                }
            }
            if world_ids.is_empty() {
                return Err(CoreError::DependencyConflict);
            }
            let mut world_ids: Vec<_> = world_ids.into_iter().collect();
            world_ids.sort();
            for world in world_ids {
                files.extend(super::worlds::selected(
                    &project, &version, &world, dependency,
                )?);
            }
        } else {
            files.extend(selected_files(&project, &version, dependency)?);
        }
        selected.insert(id, version);
    }
    // A shared dependency can be visited before a deeper parent's world is known.
    // Propagate destinations through the complete selected graph, bounded by receipt size.
    if updating {
        let mut scopes = std::collections::HashSet::new();
        let mut queue = VecDeque::new();
        for record in &files {
            if super::worlds::datapack_directory(&record.directory) {
                let world = record
                    .directory
                    .split('/')
                    .nth(1)
                    .ok_or(CoreError::UnsafePath)?
                    .to_string();
                let pair = (record.project_id.clone(), world);
                if scopes.insert(pair.clone()) {
                    queue.push_back(pair);
                }
            }
        }
        while let Some((id, world)) = queue.pop_front() {
            if scopes.len() > 4096 {
                return Err(CoreError::DependencyConflict);
            }
            for dependency in selected[&id]
                .dependencies
                .iter()
                .filter(|d| d.dependency_type == "required")
            {
                let child = dependency
                    .project_id
                    .as_ref()
                    .or_else(|| {
                        selected
                            .iter()
                            .find(|(_, v)| dependency.version_id.as_ref() == Some(&v.id))
                            .map(|(id, _)| id)
                    })
                    .ok_or(CoreError::DependencyConflict)?;
                if selected
                    .get(child)
                    .is_some_and(|v| v.loaders.iter().any(|l| l == "datapack"))
                {
                    let pair = (child.clone(), world.clone());
                    if scopes.insert(pair.clone()) {
                        queue.push_back(pair);
                    }
                }
            }
        }
        let mut scopes: Vec<_> = scopes.into_iter().collect();
        scopes.sort();
        for (id, world) in scopes {
            if !files
                .iter()
                .any(|r| r.project_id == id && r.directory == format!("saves/{world}/datapacks"))
            {
                files.extend(super::worlds::selected(
                    &provider.project(&id)?,
                    &selected[&id],
                    &world,
                    true,
                )?);
            }
        }
    }
    // Check conflicts in both directions, including already installed managed projects.
    let retained: Vec<_> = installed
        .iter()
        .filter(|r| !selected.contains_key(&r.project_id))
        .collect();
    for version in selected.values().chain(retained.iter().map(|r| &r.version)) {
        for dep in version
            .dependencies
            .iter()
            .filter(|d| d.dependency_type == "incompatible")
        {
            if dep.project_id.is_none() && dep.version_id.is_none() {
                return Err(CoreError::DependencyConflict);
            }
            let conflicts = selected
                .values()
                .chain(retained.iter().map(|r| &r.version))
                .any(|other| {
                    dep.project_id
                        .as_ref()
                        .is_none_or(|id| id == &other.project_id)
                        && dep.version_id.as_ref().is_none_or(|id| id == &other.id)
                });
            if conflicts {
                return Err(CoreError::DependencyConflict);
            }
        }
    }
    // A dependency update must not invalidate a retained project's exact requirement.
    for record in &retained {
        for dep in record
            .version
            .dependencies
            .iter()
            .filter(|d| d.dependency_type == "required")
        {
            if let Some(id) = &dep.version_id {
                let required = provider.version(id)?;
                if selected
                    .get(&required.project_id)
                    .is_some_and(|v| v.id != *id)
                {
                    return Err(CoreError::DependencyConflict);
                }
            }
        }
    }
    let mut targets = std::collections::HashSet::new();
    let mut unique_files = vec![];
    for file in files {
        if let Some(old) = unique_files.iter().find(|r: &&ContentRecord| {
            r.directory == file.directory
                && r.file.filename.eq_ignore_ascii_case(&file.file.filename)
        }) {
            if serde_json::to_vec(old)? != serde_json::to_vec(&file)? {
                return Err(CoreError::ContentConflict);
            }
        } else {
            unique_files.push(file);
        }
    }
    let files = unique_files;
    for file in &files {
        if !targets.insert(format!("{}/{}", file.directory, file.file.filename).to_lowercase()) {
            return Err(CoreError::ContentConflict);
        }
    }
    let total_bytes = files.iter().map(|r| r.file.size).sum();
    Ok(ContentPlan {
        new_instance: None,
        token: uuid::Uuid::new_v4().to_string(),
        instance_id: instance.id.clone(),
        files,
        optional_dependencies: optional,
        already_installed: 0,
        total_bytes,
    })
}
