use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const INSTANCE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Loader {
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InstanceType {
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
    ManagedProject,
    CreatorStudio,
}

impl From<Loader> for InstanceType {
    fn from(loader: Loader) -> Self {
        match loader {
            Loader::Vanilla => Self::Vanilla,
            Loader::Fabric => Self::Fabric,
            Loader::Forge => Self::Forge,
            Loader::NeoForge => Self::NeoForge,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InstallStatus {
    NotInstalled,
    Installed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AutoMode {
    Auto,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum IconSource {
    #[default]
    Automatic,
    Builtin,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedShortcut {
    pub file_name: String,
    pub icon_revision: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutPlan {
    pub instance_id: String,
    pub arguments: Vec<String>,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Instance {
    pub schema_version: u32,
    pub id: String,
    pub revision: u32,
    pub name: String,
    pub minecraft_version: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    pub instance_type: InstanceType,
    pub collection_id: Option<String>,
    pub status: InstallStatus,
    pub java_mode: AutoMode,
    pub memory_mode: AutoMode,
    pub last_account_id: Option<String>,
    // Read compatibility for intermediate development fixtures; never used for selection.
    #[serde(default, skip_serializing)]
    #[ts(skip)]
    pub default_account_id: Option<String>,
    #[ts(type = "number")]
    pub created_at: u64,
    #[ts(type = "number")]
    pub updated_at: u64,
    #[ts(type = "number | null")]
    pub last_played_at: Option<u64>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub icon_source: IconSource,
    #[serde(default)]
    pub managed_shortcut: Option<ManagedShortcut>,
    pub icon_ref: Option<String>,
    pub cover_ref: Option<String>,
    pub provider_refs: Vec<String>,
    pub installed_content_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Accent {
    Sage,
    Amber,
    Rose,
    Teal,
    Slate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CollectionIcon {
    Folder,
    Leaf,
    Camera,
    Users,
    Archive,
    Flask,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Collection {
    pub schema_version: u32,
    pub id: String,
    pub revision: u32,
    pub name: String,
    pub description: String,
    pub accent: Accent,
    pub icon: CollectionIcon,
    pub order: u32,
    pub cover_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    pub instances: Vec<Instance>,
    pub collections: Vec<Collection>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LibraryChange {
    pub snapshot: LibrarySnapshot,
    pub affected_id: String,
    pub preserved_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateInstance {
    pub name: String,
    pub minecraft_version: String,
    pub loader: Loader,
    pub collection_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateInstance {
    pub id: String,
    pub expected_revision: u32,
    pub name: String,
    pub collection_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigureLaunch {
    pub id: String,
    pub expected_revision: u32,
    pub loader_version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DuplicateInstance {
    pub id: String,
    pub expected_revision: u32,
    pub name: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DeleteMode {
    PreserveWorlds,
    Everything,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteInstance {
    pub id: String,
    pub expected_revision: u32,
    pub mode: DeleteMode,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveCollection {
    pub id: Option<String>,
    pub expected_revision: Option<u32>,
    pub name: String,
    pub description: String,
    pub accent: Accent,
    pub icon: CollectionIcon,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordRequest {
    pub id: String,
    pub expected_revision: u32,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetFavorite {
    pub id: String,
    pub expected_revision: u32,
    pub favorite: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FolderTarget {
    Data,
    Instance,
    Mods,
    Logs,
    Backup,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenFolder {
    pub target: FolderTarget,
    pub id: Option<String>,
}

pub fn valid_id(id: &str) -> Result<(), CoreError> {
    if uuid::Uuid::parse_str(id).is_ok_and(|parsed| parsed.to_string() == id) {
        Ok(())
    } else {
        Err(CoreError::InvalidInput)
    }
}

pub fn display_name(value: &str) -> Result<String, CoreError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 80 || value.chars().any(char::is_control) {
        return Err(CoreError::InvalidInput);
    }
    Ok(value.to_owned())
}

pub fn game_version(value: &str) -> Result<String, CoreError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 64
        || value == "."
        || value.contains("..")
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_' | b' '))
    {
        return Err(CoreError::InvalidInput);
    }
    Ok(value.to_owned())
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn next_revision(value: u32) -> Result<u32, CoreError> {
    value.checked_add(1).ok_or(CoreError::InvalidInput)
}

impl Instance {
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.schema_version > INSTANCE_SCHEMA {
            return Err(CoreError::SchemaTooNew);
        }
        if self.schema_version != INSTANCE_SCHEMA {
            return Err(CoreError::InvalidInput);
        }
        valid_id(&self.id)?;
        display_name(&self.name)?;
        game_version(&self.minecraft_version)?;
        if let Some(id) = &self.collection_id {
            valid_id(id)?;
        }
        Ok(())
    }
}
