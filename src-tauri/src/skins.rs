use crate::{
    error::CoreError,
    game::{
        fs::{read_limited, write_atomic},
        local_profile::valid_nickname,
        network::Network,
    },
    instances::{filesystem::Paths, model::now},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SkinStatus {
    Found,
    Missing,
    Unavailable,
    Cached,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SkinView {
    pub status: SkinStatus,
    pub png: Option<String>,
    pub slim: bool,
    #[ts(type = "number")]
    pub fetched_at: u64,
}

fn fetch(nickname: &str) -> Result<SkinView, CoreError> {
    let network = Network::with_timeout(std::time::Duration::from_secs(10))?;
    let Some(profile) = network.optional_bytes(
        &format!("https://api.mojang.com/users/profiles/minecraft/{nickname}"),
        100_000,
    )?
    else {
        return Ok(SkinView {
            status: SkinStatus::Missing,
            png: None,
            slim: false,
            fetched_at: now(),
        });
    };
    let profile: Value = serde_json::from_slice(&profile)?;
    let id = profile["id"].as_str().ok_or(CoreError::Integrity)?;
    let uuid = uuid::Uuid::parse_str(id).map_err(|_| CoreError::Integrity)?;
    if !profile["name"]
        .as_str()
        .is_some_and(|name| name.eq_ignore_ascii_case(nickname))
    {
        return Err(CoreError::Integrity);
    }
    let value: Value = serde_json::from_slice(&network.bytes(
        &format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{}",
            uuid.simple()
        ),
        100_000,
    )?)?;
    let encoded = value["properties"]
        .as_array()
        .and_then(|props| props.iter().find(|p| p["name"] == "textures"))
        .and_then(|p| p["value"].as_str())
        .ok_or(CoreError::Integrity)?;
    let texture: Value =
        serde_json::from_slice(&STANDARD.decode(encoded).map_err(|_| CoreError::Integrity)?)?;
    if texture["profileId"].as_str() != Some(id) {
        return Err(CoreError::Integrity);
    }
    let Some(url) = texture
        .pointer("/textures/SKIN/url")
        .and_then(Value::as_str)
    else {
        return Ok(SkinView {
            status: SkinStatus::Missing,
            png: None,
            slim: false,
            fetched_at: now(),
        });
    };
    let mut url = reqwest::Url::parse(url).map_err(|_| CoreError::Integrity)?;
    if url.host_str() != Some("textures.minecraft.net") || !url.path().starts_with("/texture/") {
        return Err(CoreError::Integrity);
    }
    url.set_scheme("https").map_err(|_| CoreError::Integrity)?;
    let png = network.bytes(url.as_str(), 2_000_000)?;
    validate_png(&png)?;
    Ok(SkinView {
        status: SkinStatus::Found,
        png: Some(format!("data:image/png;base64,{}", STANDARD.encode(png))),
        slim: texture
            .pointer("/textures/SKIN/metadata/model")
            .and_then(Value::as_str)
            == Some("slim"),
        fetched_at: now(),
    })
}
fn validate_png(png: &[u8]) -> Result<(), CoreError> {
    if png.len() < 33 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return Err(CoreError::Integrity);
    }
    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
    if width != 64 || !matches!(height, 32 | 64) {
        return Err(CoreError::Integrity);
    }
    Ok(())
}
pub fn lookup(root: &Path, nickname: &str, refresh: bool) -> Result<SkinView, CoreError> {
    if !valid_nickname(nickname) {
        return Err(CoreError::InvalidInput);
    }
    let paths = Paths::new(root)?;
    let path = paths
        .root()
        .join("shared/cache/skins")
        .join(format!("{}.json", nickname.to_ascii_lowercase()));
    let cached = read_limited(&paths, &path, 3_000_000)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SkinView>(&bytes).ok())
        .filter(|skin| {
            skin.png.as_ref().is_none_or(|png| {
                png.strip_prefix("data:image/png;base64,")
                    .and_then(|s| STANDARD.decode(s).ok())
                    .is_some_and(|png| validate_png(&png).is_ok())
            })
        });
    if !refresh
        && let Some(skin) = &cached
        && now().saturating_sub(skin.fetched_at)
            < if skin.png.is_some() {
                86_400_000
            } else {
                900_000
            }
    {
        return Ok(skin.clone());
    }
    match fetch(nickname) {
        Ok(skin) => {
            write_atomic(&paths, &path, &serde_json::to_vec(&skin)?)?;
            Ok(skin)
        }
        Err(_) => Ok(cached
            .map(|mut s| {
                s.status = SkinStatus::Cached;
                s
            })
            .unwrap_or(SkinView {
                status: SkinStatus::Unavailable,
                png: None,
                slim: false,
                fetched_at: now(),
            })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_non_skin_images() {
        assert!(validate_png(b"<svg></svg>").is_err());
        let mut png = vec![0; 33];
        png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png[12..16].copy_from_slice(b"IHDR");
        png[16..20].copy_from_slice(&64u32.to_be_bytes());
        png[20..24].copy_from_slice(&64u32.to_be_bytes());
        assert!(validate_png(&png).is_ok());
        png[16..20].copy_from_slice(&5000u32.to_be_bytes());
        assert!(validate_png(&png).is_err());
    }
}
