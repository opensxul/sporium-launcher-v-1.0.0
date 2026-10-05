use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::CoreError;

pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Locale {
    #[default]
    #[serde(rename = "ru-RU")]
    Russian,
    #[serde(rename = "en-US")]
    English,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Motion {
    #[default]
    System,
    Reduced,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub schema_version: u32,
    pub locale: Locale,
    pub motion: Motion,
    pub ui_scale: u16,
    #[serde(default)]
    pub version_visibility: crate::game::model::VersionVisibility,
    #[serde(default)]
    pub custom_java_path: Option<String>,
    #[serde(default = "crate::game::local_profile::default_nickname")]
    pub local_nickname: String,
    #[serde(default = "skins_enabled")]
    pub nickname_skins: bool,
    #[serde(default = "download_concurrency")]
    pub download_concurrency: u8,
    #[serde(default)]
    pub cache_limit_mb: u32,
}
fn download_concurrency() -> u8 {
    6
}

fn skins_enabled() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            locale: Locale::default(),
            motion: Motion::default(),
            ui_scale: 100,
            version_visibility: Default::default(),
            custom_java_path: None,
            local_nickname: crate::game::local_profile::default_nickname(),
            nickname_skins: true,
            download_concurrency: 6,
            cache_limit_mb: 0,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), CoreError> {
        if !(1..=12).contains(&self.download_concurrency)
            || self.cache_limit_mb > 102400
            || (self.cache_limit_mb > 0 && self.cache_limit_mb < 64)
        {
            return Err(CoreError::InvalidSettings);
        }
        if !crate::game::local_profile::valid_nickname(&self.local_nickname) {
            return Err(CoreError::InvalidSettings);
        }
        if let Some(path) = &self.custom_java_path
            && (path.len() > 2048
                || !std::path::Path::new(path).is_absolute()
                || !std::path::Path::new(path)
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("java.exe")))
        {
            return Err(CoreError::InvalidSettings);
        }
        if self.schema_version > SETTINGS_SCHEMA_VERSION {
            return Err(CoreError::SchemaTooNew);
        }
        if self.schema_version != SETTINGS_SCHEMA_VERSION
            || !matches!(self.ui_scale, 90 | 100 | 110 | 125)
        {
            return Err(CoreError::InvalidSettings);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsSnapshot {
    pub values: Settings,
    pub revision: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveSettingsRequest {
    pub values: Settings,
    pub expected_revision: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_settings_gain_a_local_profile_without_resetting_preferences() {
        let settings: Settings = serde_json::from_str(
            r#"{"schemaVersion":1,"locale":"en-US","motion":"reduced","uiScale":125}"#,
        )
        .unwrap();
        assert_eq!(settings.local_nickname, "SporiumLocal");
        assert_eq!(settings.locale, Locale::English);
        assert_eq!(settings.ui_scale, 125);
        settings.validate().unwrap();
        let invalid = Settings {
            local_nickname: "invalid name".into(),
            ..settings
        };
        assert!(invalid.validate().is_err());
    }
}
