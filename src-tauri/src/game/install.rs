use super::{
    GameManager, catalog,
    fs::{extract, read_limited, write_atomic},
    java,
    launch::{self, LaunchInputs, ResolvedLaunchPlan},
    metadata,
    model::JobPhase,
    network::Download,
    rules::Environment,
};
use crate::{
    error::CoreError,
    instances::{filesystem::Paths, model::Instance},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

pub fn prepare(
    manager: &GameManager,
    instance: &Instance,
    directory: &Path,
    nickname: &str,
) -> Result<ResolvedLaunchPlan, CoreError> {
    let paths = Paths::new(manager.library.root())?;
    let _download_lock = paths.named_lock(&paths.root().join("shared/cache/downloads.lock"))?;
    let cancelled = || manager.cancelled();
    let progress = |bytes| manager.bytes(bytes);
    let network = super::network::Network::new()?;
    manager.phase(JobPhase::Resolving);
    let manifest = catalog::for_version(&paths, &network, &instance.minecraft_version)?;
    let entry = manifest
        .versions
        .iter()
        .find(|entry| entry.id == instance.minecraft_version)
        .ok_or(CoreError::UnsupportedVersion)?;
    let metadata = metadata::version(&paths, &network, entry)?;
    let java_major = metadata
        .pointer("/javaVersion/majorVersion")
        .and_then(Value::as_u64)
        .and_then(|major| u32::try_from(major).ok())
        .ok_or(CoreError::UnsupportedVersion)?;
    let settings = manager.database.load_settings()?;
    manager.phase(JobPhase::Java);
    let runtime = java::resolve(
        &paths,
        &network,
        java_major,
        settings.values.custom_java_path.as_deref(),
        &cancelled,
        &progress,
    )?;
    let (metadata, loader_version) =
        super::loaders::prepare(manager, instance, &metadata, &runtime, directory)?;
    let mut libraries = metadata::libraries(&paths, &metadata, &Environment::windows(false))?;
    let assets = metadata::assets(
        &paths, &network, &metadata, directory, &cancelled, &progress,
    )?;
    let logging = if let Some(value) = metadata.pointer("/logging/client/file") {
        let id = metadata::string(value, "id")?;
        let path = paths
            .root()
            .join("shared/assets/log_configs")
            .join(super::fs::relative(id)?);
        libraries
            .downloads
            .push(metadata::artifact(value, path.clone())?);
        Some(path)
    } else {
        None
    };
    libraries.downloads.extend(assets.downloads);
    download_all(manager, &paths, &network, &libraries.downloads)?;
    manager.phase(JobPhase::Extracting);
    let staging = paths
        .root()
        .join("launcher/staging")
        .join(format!("natives-{}", uuid::Uuid::new_v4()));
    let natives = directory.join(".sporium/natives").join(&entry.sha1);
    // Re-extract into a fresh directory on every repair/launch; never trust stale DLLs.
    let extraction = (|| {
        paths.mkdir(&staging)?;
        for (source, excludes) in &libraries.natives {
            extract(&paths, source, &staging, excludes, &cancelled)?;
        }
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        paths.mkdir(natives.parent().ok_or(CoreError::UnsafePath)?)?;
        paths.remove(&natives)?;
        paths.rename(&staging, &natives)?;
        Ok(())
    })();
    let _ = paths.remove(&staging);
    extraction?;
    for child in ["java", "jna", "lwjgl", "netty"] {
        paths.mkdir(&natives.join(child))?;
    }
    for (source, destination) in &assets.virtual_files {
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        copy_asset(&paths, source, destination)?;
    }
    manager.phase(JobPhase::Verifying);
    // Cosmetic service failure must not prevent an otherwise valid game from launching.
    if let Err(error) =
        super::skinmod::prepare(manager, instance, directory, settings.values.nickname_skins)
    {
        if matches!(error, CoreError::Cancelled | CoreError::UnsafePath) {
            return Err(error);
        }
        tracing::warn!("Optional nickname skin module unavailable");
    }
    let plan = launch::resolve(LaunchInputs {
        paths: &paths,
        metadata: &metadata,
        runtime: &runtime,
        classpath: libraries.classpath,
        natives,
        game_dir: directory.into(),
        game_assets: assets.game_assets,
        asset_id: assets.id,
        logging,
        nickname,
    })?;
    write_atomic(
        &paths,
        &directory.join(".sporium/installation.json"),
        &serde_json::to_vec_pretty(
            &json!({ "schemaVersion": 1, "version": instance.minecraft_version, "manifestSha1": entry.sha1, "java": runtime, "memoryMb": plan.memory_mb }),
        )?,
    )?;
    manager
        .library
        .pin_loader(&instance.id, loader_version.as_deref())?;
    Ok(plan)
}

fn copy_asset(paths: &Paths, source: &Path, destination: &Path) -> Result<(), CoreError> {
    paths.checked(source)?;
    paths.checked(destination)?;
    // Virtual legacy asset trees are small; atomic writes avoid exposing incomplete files.
    let bytes = read_limited(paths, source, 200_000_000)?;
    if fs::read(destination).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    write_atomic(paths, destination, &bytes)
}

pub fn download_all(
    manager: &GameManager,
    paths: &Paths,
    network: &super::network::Network,
    items: &[Download],
) -> Result<(), CoreError> {
    let mut seen = std::collections::HashSet::<PathBuf>::new();
    let items = items
        .iter()
        .filter(|item| seen.insert(item.path.clone()))
        .collect::<Vec<_>>();
    manager.download_plan(items.len() as u32, items.iter().map(|item| item.size).sum());
    let concurrency = manager
        .database
        .load_settings()?
        .values
        .download_concurrency;
    let next = AtomicUsize::new(0);
    let error = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..concurrency {
            scope.spawn(|| {
                loop {
                    if manager.cancelled() || error.lock().unwrap().is_some() {
                        break;
                    }
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(index) else {
                        break;
                    };
                    manager.transfer_started(
                        index as u32,
                        item.path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into(),
                        item.size,
                    );
                    let result = network.download_observed(
                        paths,
                        item,
                        &|| manager.cancelled() || error.lock().unwrap().is_some(),
                        &|bytes| manager.bytes(bytes),
                        &|event| manager.transfer_event(index as u32, event),
                    );
                    match result {
                        Ok(outcome) => manager.transfer_done(index as u32, item.size, outcome),
                        Err(reason) => {
                            let mut error = error.lock().unwrap();
                            if error.is_none() {
                                *error = Some(reason);
                            }
                            break;
                        }
                    }
                }
            });
        }
    });
    if let Some(error) = error.into_inner().unwrap() {
        return Err(error);
    }
    if manager.cancelled() {
        return Err(CoreError::Cancelled);
    }
    Ok(())
}
