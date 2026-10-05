use crate::error::CommandError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum VersionKind {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GameVersion {
    pub id: String,
    pub kind: VersionKind,
    pub released_at: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct VersionCatalog {
    pub versions: Vec<GameVersion>,
    pub latest_release: String,
    pub latest_snapshot: String,
    pub cached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JavaRuntime {
    pub executable: String,
    pub major: u32,
    pub version: String,
    pub architecture: String,
    pub managed: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum GameAction {
    Install,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameRequest {
    pub id: String,
    pub action: GameAction,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum JobPhase {
    Interrupted,
    Resolving,
    Java,
    Loader,
    Downloading,
    Extracting,
    Verifying,
    Launching,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GameJob {
    pub action: GameAction,
    pub paused: bool,
    pub cached_files: u32,
    pub repaired_files: u32,
    pub retries: u32,
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number")]
    pub verified_bytes: u64,
    #[ts(type = "number")]
    pub bytes_per_second: u64,
    #[ts(type = "number | null")]
    pub eta_seconds: Option<u64>,
    pub active_files: Vec<TransferFile>,
    pub instance_id: String,
    pub phase: JobPhase,
    pub completed_files: u32,
    pub total_files: u32,
    #[ts(type = "number")]
    pub downloaded_bytes: u64,
    pub error: Option<CommandError>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GameSession {
    pub instance_id: String,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub log_path: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GameState {
    pub job: Option<GameJob>,
    pub sessions: Vec<GameSession>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TransferFile {
    pub index: u32,
    pub name: String,
    #[ts(type = "number")]
    pub received: u64,
    #[ts(type = "number")]
    pub total: u64,
}

impl GameJob {
    pub fn new(request: &GameRequest, phase: JobPhase) -> Self {
        Self {
            instance_id: request.id.clone(),
            action: request.action,
            phase,
            paused: false,
            cached_files: 0,
            repaired_files: 0,
            retries: 0,
            total_bytes: 0,
            verified_bytes: 0,
            bytes_per_second: 0,
            eta_seconds: None,
            active_files: vec![],
            completed_files: 0,
            total_files: 0,
            downloaded_bytes: 0,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct VersionVisibility {
    pub releases: bool,
    pub snapshots: bool,
    pub beta: bool,
    pub alpha: bool,
    pub other: bool,
}
impl Default for VersionVisibility {
    fn default() -> Self {
        Self {
            releases: true,
            snapshots: false,
            beta: false,
            alpha: false,
            other: false,
        }
    }
}
