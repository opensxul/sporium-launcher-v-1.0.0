use crate::{error::CommandError, instances::model::Loader};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackFile {
    pub path: String,
    pub hashes: BTreeMap<String, String>,
    pub downloads: Vec<String>,
    pub file_size: u64,
    #[serde(default)]
    pub env: Option<Environment>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub client: String,
    pub server: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format_version: u32,
    pub game: String,
    pub version_id: String,
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
    pub dependencies: BTreeMap<String, String>,
    pub files: Vec<PackFile>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SporiumPack {
    pub schema_version: u32,
    pub manifest: Manifest,
    // Hashes cover embedded overrides, including empty files. No executable launch commands.
    pub overrides: BTreeMap<String, EmbeddedFile>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddedFile {
    pub sha512: String,
    pub size: u64,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PackPreview {
    pub token: String,
    pub source: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub minecraft: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    pub required_files: u32,
    pub optional_files: Vec<String>,
    #[ts(type = "number")]
    pub download_bytes: u64,
    #[ts(type = "number")]
    pub embedded_bytes: u64,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PackJob {
    pub name: String,
    pub phase: String,
    pub instance_id: Option<String>,
    pub completed_files: u32,
    pub total_files: u32,
    #[ts(type = "number")]
    pub downloaded_bytes: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
    pub error: Option<CommandError>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PackExport {
    pub file_name: String,
    pub referenced_files: u32,
    pub embedded_files: u32,
    pub omitted: Vec<String>,
}
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackExportRequest {
    pub id: String,
    pub include_worlds: bool,
    pub include_local: bool,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExternalCandidate {
    pub key: String,
    pub name: String,
    pub minecraft: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    pub source: String,
    pub warnings: Vec<String>,
}
