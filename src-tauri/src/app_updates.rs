use crate::{
    commands::AppState,
    error::{CommandError, CoreError},
    instances::{filesystem::Paths, repository},
};
use serde::Serialize;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};
use ts_rs::TS;

#[derive(Default)]
pub struct Updates {
    pending: Mutex<Option<Update>>,
    pub busy: AtomicBool,
}
#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdate {
    pub version: String,
    pub notes: String,
    #[ts(type = "number | null")]
    pub size: Option<u64>,
}

#[tauri::command]
pub async fn app_update_check(app: tauri::AppHandle) -> Option<AppUpdate> {
    // Offline, missing releases and current versions produce no visible check UI.
    let updater = app
        .updater_builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .ok()?;
    let update = updater.check().await.ok()??;
    if update.download_url.scheme() != "https"
        || update.download_url.host_str() != Some("github.com")
        || !update
            .download_url
            .path()
            .starts_with("/opensxul/sporium-launcher-v-1.0.0/releases/download/")
    {
        return None;
    }
    let info = AppUpdate {
        version: update.version.clone(),
        notes: update
            .body
            .clone()
            .unwrap_or_default()
            .chars()
            .take(16_000)
            .collect(),
        size: update.raw_json.get("size").and_then(|value| value.as_u64()),
    };
    *app.state::<Updates>().pending.lock().ok()? = Some(update);
    Some(info)
}

fn active(state: &AppState) -> bool {
    let game = state.game.snapshot();
    state.game.is_active()
        || game.job.as_ref().is_some_and(|job| job.paused)
        || state.content.is_active()
        || state.packs.is_active()
        || state.projects.is_active()
}
struct BusyGuard<'a>(&'a AtomicBool);
impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn app_update_install(app: tauri::AppHandle) -> Result<(), CommandError> {
    let updates = app.state::<Updates>();
    if updates.busy.swap(true, Ordering::SeqCst) {
        return Err(CoreError::InstanceBusy.into());
    }
    let _guard = BusyGuard(&updates.busy);
    let state = app.state::<AppState>();
    if active(&state) {
        return Err(CoreError::InstanceBusy.into());
    }
    let mut update = updates
        .pending
        .lock()
        .map_err(|_| CoreError::Worker)?
        .clone()
        .ok_or(CoreError::NotFound)?;
    update.timeout = Some(std::time::Duration::from_secs(300));
    let mut received = 0u64;
    let mut last = std::time::Instant::now();
    let bytes = update
        .download(
            |length, total| {
                received += length as u64;
                if last.elapsed().as_millis() >= 100 || total == Some(received) {
                    let _ = app.emit(
                        "app-update-progress",
                        serde_json::json!({"received":received,"total":total}),
                    );
                    last = std::time::Instant::now();
                }
            },
            || {},
        )
        .await
        .map_err(|error| match error {
            tauri_plugin_updater::Error::Reqwest(_) | tauri_plugin_updater::Error::Network(_) => {
                CoreError::Network
            }
            tauri_plugin_updater::Error::Io(error) => CoreError::Io(error),
            _ => CoreError::Integrity,
        })?;
    if active(&state) {
        return Err(CoreError::InstanceBusy.into());
    }
    // Freeze instance mutations and refuse active leases, including another launcher process.
    // Download/verification finishes before taking these short-lived local locks.
    let paths = Paths::new(state.library.root())?;
    let _library = paths.lock()?;
    let db = state.database.connect()?;
    let snapshot = repository::snapshot(&db)?;
    let _leases = snapshot
        .instances
        .iter()
        .map(|instance| paths.instance_lock(&instance.id))
        .collect::<Result<Vec<_>, _>>()?;
    if active(&state) {
        return Err(CoreError::InstanceBusy.into());
    }
    update.install(bytes).map_err(|_| CoreError::Worker)?;
    Ok(())
}
