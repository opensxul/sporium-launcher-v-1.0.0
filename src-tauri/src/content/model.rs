use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogQuery {
    pub query: String,
    pub kind: String,
    pub minecraft: String,
    pub loader: String,
    pub category: String,
    pub environment: String,
    pub sort: String,
    pub offset: u32,
    pub instance_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContentHit {
    #[serde(default)]
    pub icon_url: Option<String>,
    pub id: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub kind: String,
    #[ts(type = "number")]
    pub downloads: u64,
    pub categories: Vec<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPage {
    pub hits: Vec<ContentHit>,
    pub next_offset: Option<u32>,
    pub total: u32,
    pub cached: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ContentCategory {
    pub name: String,
    pub project_type: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ContentLoader {
    pub name: String,
    pub supported_project_types: Vec<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContentTags {
    pub categories: Vec<ContentCategory>,
    pub loaders: Vec<ContentLoader>,
    pub game_versions: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ContentLicense {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ContentProject {
    #[serde(default)]
    pub icon_url: Option<String>,
    pub id: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub project_type: String,
    pub license: ContentLicense,
    pub client_side: String,
    pub server_side: String,
    #[serde(default)]
    pub environment: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ContentDependency {
    pub version_id: Option<String>,
    pub project_id: Option<String>,
    pub file_name: Option<String>,
    pub dependency_type: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ContentHashes {
    pub sha1: String,
    pub sha512: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ContentFile {
    #[serde(default)]
    pub id: Option<String>,
    pub filename: String,
    pub url: String,
    pub hashes: ContentHashes,
    #[ts(type = "number")]
    pub size: u64,
    pub primary: bool,
    pub file_type: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ContentVersion {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub version_type: String,
    pub date_published: String,
    pub status: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    #[serde(default)]
    pub environment: String,
    pub dependencies: Vec<ContentDependency>,
    pub files: Vec<ContentFile>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContentDetails {
    pub project: ContentProject,
    pub versions: Vec<ContentVersion>,
    pub compatible_instances: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentRequest {
    pub instance_id: String,
    pub project_id: String,
    pub version_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContentRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<LocalMetadata>,
    #[serde(default)]
    pub icon_url: Option<String>,
    pub provider: String,
    pub project_id: String,
    pub title: String,
    pub kind: String,
    pub version: ContentVersion,
    pub file: ContentFile,
    pub directory: String,
    pub dependency: bool,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContentPlan {
    pub new_instance: Option<crate::instances::model::CreateInstance>,
    pub token: String,
    pub instance_id: String,
    pub files: Vec<ContentRecord>,
    pub optional_dependencies: u32,
    pub already_installed: u32,
    #[ts(type = "number")]
    pub total_bytes: u64,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContentJob {
    pub instance_id: String,
    pub phase: String,
    pub completed_files: u32,
    pub total_files: u32,
    #[ts(type = "number")]
    pub downloaded_bytes: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
    pub current_file: Option<String>,
    pub error: Option<crate::error::CommandError>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct InstalledContent {
    pub record: ContentRecord,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UntrackedContent {
    pub manageable: bool,
    pub directory: String,
    pub filename: String,
    pub kind: String,
    pub title: String,
    pub version: String,
    pub metadata: Option<LocalMetadata>,
    pub status: String,
    pub disabled: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentAdoptionRequest {
    pub instance_id: String,
    pub directory: String,
    pub filename: String,
    pub recognize: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentCreateRequest {
    pub project_id: String,
    pub version_id: String,
    pub instance: crate::instances::model::CreateInstance,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContentAction {
    Enable,
    Disable,
    Delete,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentSelection {
    pub directory: String,
    pub filename: String,
    pub sha512: String,
}
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentChange {
    pub instance_id: String,
    pub action: ContentAction,
    pub files: Vec<ContentSelection>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContentHistory {
    pub id: String,
    #[ts(type = "number")]
    pub timestamp: u64,
    pub action: String,
    pub titles: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LocalMetadata {
    #[serde(default)]
    pub versions: Vec<LocalModVersion>,
    #[serde(default)]
    pub dependencies: Vec<LocalDependency>,
    pub mod_ids: Vec<String>,
    pub loader: String,
    pub minecraft: Vec<String>,
    pub required: Vec<String>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LocalModVersion {
    pub id: String,
    pub version: Option<String>,
    pub alias: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LocalDependency {
    pub owner_id: String,
    pub id: String,
    pub relation: String,
    pub ranges: Vec<String>,
    pub dialect: String,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DependencyTarget {
    pub title: String,
    pub version: Option<String>,
    pub directory: String,
    pub filename: String,
    pub enabled: bool,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DependencyCheck {
    pub dependency: LocalDependency,
    pub status: String,
    pub severity: String,
    pub targets: Vec<DependencyTarget>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModDiagnostic {
    pub title: String,
    pub directory: String,
    pub filename: String,
    pub enabled: bool,
    pub status: String,
    pub warnings: Vec<String>,
    pub checks: Vec<DependencyCheck>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModDiagnostics {
    pub complete: bool,
    pub errors: u32,
    pub warnings: u32,
    pub mods: Vec<ModDiagnostic>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LocalContentPlan {
    pub diagnostics: ModDiagnostics,
    pub plan: ContentPlan,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorldImport {
    pub id: String,
    pub title: String,
    pub source: String,
    pub sha512: String,
    #[ts(type = "number")]
    pub imported_at: u64,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContentWorld {
    pub id: String,
    pub title: String,
    pub imported: Option<WorldImport>,
}
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorldArchiveRequest {
    pub instance_id: String,
    pub source: String,
    pub kind: String,
    pub world: Option<String>,
    pub title: String,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorldArchivePlan {
    pub token: String,
    pub instance_id: String,
    pub kind: String,
    pub title: String,
    pub world: String,
    pub source: String,
    #[ts(type = "number")]
    pub bytes: u64,
    pub files: u32,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorldProjectRequest {
    pub instance_id: String,
    pub project_id: String,
    pub version_id: String,
    pub world: String,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LocalDependencyPlan {
    pub plan: ContentPlan,
    pub parents: Vec<ContentRecord>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContentAdoptionPlan {
    pub diagnostics: ModDiagnostics,
    pub plan: ContentPlan,
    pub warnings: Vec<String>,
    pub metadata: LocalMetadata,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentUpdatePolicy {
    pub project_id: String,
    pub pinned: bool,
    pub ignored_version: Option<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContentUpdate {
    pub project_id: String,
    pub title: String,
    pub current_version: String,
    pub status: String,
    pub candidate: Option<ContentVersion>,
    pub policy: ContentUpdatePolicy,
}
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentUpdateRequest {
    pub instance_id: String,
    pub files: Vec<ContentSelection>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContentUpdatePlan {
    pub plan: ContentPlan,
    pub previous: Vec<ContentRecord>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContentRestorePoint {
    pub id: String,
    #[ts(type = "number")]
    pub timestamp: u64,
    pub title: String,
    pub files: u32,
    pub available: bool,
    pub settings_available: bool,
    pub project_point: bool,
}
