use serde::Serialize;
use ts_rs::TS;

use crate::{
    error::{CommandError, CoreError},
    settings::{SaveSettingsRequest, SettingsSnapshot},
    storage::{Database, migrations::SCHEMA_VERSION},
};

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub platform: String,
    pub database_schema: u32,
    pub data_directory: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub info: AppInfo,
    pub settings: SettingsSnapshot,
    pub profiles: crate::profiles::ProfileSnapshot,
}

pub struct AppState {
    pub projects: crate::projects::ProjectManager,
    pub packs: crate::packs::PackManager,
    pub content: crate::content::ContentManager,
    pub database: Database,
    pub library: crate::instances::Library,
    pub game: crate::game::GameManager,
    pub info: AppInfo,
    // Retained for the app lifetime to flush the background logging writer on shutdown.
    pub _log_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

#[tauri::command]
pub async fn bootstrap(state: tauri::State<'_, AppState>) -> Result<Bootstrap, CommandError> {
    let database = state.database.clone();
    let info = state.info.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let settings = database.load_settings()?;
        let profiles = crate::profiles::snapshot(&database)?;
        Ok(Bootstrap {
            info,
            settings,
            profiles,
        })
    })
    .await
    .map_err(|_| CommandError::from(CoreError::Worker))?
}

#[tauri::command]
pub async fn save_settings(
    request: SaveSettingsRequest,
    state: tauri::State<'_, AppState>,
) -> Result<SettingsSnapshot, CommandError> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        database.save_settings(request).map_err(CommandError::from)
    })
    .await
    .map_err(|_| CommandError::from(CoreError::Worker))?
}

impl AppInfo {
    pub fn new(data_directory: String) -> Self {
        Self {
            name: "Sporium".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            platform: std::env::consts::OS.into(),
            database_schema: SCHEMA_VERSION,
            data_directory,
        }
    }
}
