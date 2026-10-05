use crate::{instances::model::Loader, projects::transaction::Blob};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FilePolicy {
    RequiredLocked,
    Optional,
    UserAllowed,
    UserForbidden,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectFile {
    pub path: String,
    pub sha256: String,
    pub sha512: String,
    #[ts(type = "number")]
    pub size: u64,
    pub source: Option<String>,
    pub policy: FilePolicy,
    pub group: Option<String>,
}
impl ProjectFile {
    pub(crate) fn blob(&self) -> Blob {
        Blob {
            sha256: self.sha256.clone(),
            sha512: self.sha512.clone(),
            size: self.size,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackSource {
    pub project_id: String,
    pub version_id: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectLaunch {
    pub memory_mib: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectManifest {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub name: String,
    pub minecraft: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    pub creator_studio: bool,
    pub forbid_external_mods: bool,
    pub files: Vec<ProjectFile>,
    pub optional_groups: Vec<String>,
    pub launch: ProjectLaunch,
    pub pack_source: Option<PackSource>,
}
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StudioRequest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub forbid_external_mods: bool,
    pub policies: Vec<StudioFilePolicy>,
}
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StudioFilePolicy {
    pub path: String,
    pub policy: FilePolicy,
    pub group: Option<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub manifest: Option<ProjectManifest>,
    pub pack_source: Option<PackSource>,
    pub local_source: Option<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdate {
    pub current: String,
    pub available: Option<String>,
    pub version_id: Option<String>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPlan {
    pub token: String,
    pub instance_id: String,
    pub action: String,
    pub current: String,
    pub available: String,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
    pub optional_groups: Vec<String>,
    pub selected_groups: Vec<String>,
}
