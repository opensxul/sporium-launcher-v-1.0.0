//! Read bounded PNGs from archives, without extracting or executing their contents.
use crate::error::CoreError;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{fs::File, io::Read, path::Path};

fn entry(
    zip: &mut zip::ZipArchive<File>,
    name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>, CoreError> {
    let count = zip.file_names().filter(|n| *n == name).count();
    if count == 0 {
        return Ok(None);
    }
    if count != 1 {
        return Err(CoreError::Integrity);
    }
    let file = zip.by_name(name).map_err(|_| CoreError::Integrity)?;
    if file.size() > limit {
        return Err(CoreError::Integrity);
    }
    let mut bytes = vec![];
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(CoreError::Integrity);
    }
    Ok(Some(bytes))
}

pub(super) fn read(path: &Path, folder: &str) -> Result<Option<String>, CoreError> {
    let input = File::open(path)?;
    if input.metadata()?.len() > 200_000_000 {
        return Err(CoreError::Integrity);
    }
    let mut zip = zip::ZipArchive::new(input).map_err(|_| CoreError::Integrity)?;
    if zip.len() > 100_000 {
        return Err(CoreError::Integrity);
    }
    let mut name = if matches!(folder, "resourcepacks" | "shaderpacks") {
        Some("pack.png".to_string())
    } else {
        None
    };
    if name.is_none()
        && let Some(bytes) = entry(&mut zip, "fabric.mod.json", 524_288)?
    {
        let json: serde_json::Value = serde_json::from_slice(&bytes)?;
        name = json.get("icon").and_then(|icon| {
            icon.as_str().map(String::from).or_else(|| {
                icon.as_object().and_then(|sizes| {
                    sizes
                        .iter()
                        .filter_map(|(size, value)| {
                            Some((size.parse::<u32>().ok()?, value.as_str()?))
                        })
                        .filter(|(size, _)| *size <= 1024)
                        .max_by_key(|(size, _)| *size)
                        .map(|(_, value)| value.to_string())
                })
            })
        });
    }
    for metadata in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if name.is_none()
            && let Some(bytes) = entry(&mut zip, metadata, 524_288)?
        {
            let data = std::str::from_utf8(&bytes).map_err(|_| CoreError::Integrity)?;
            let table: toml::Value = toml::from_str(data).map_err(|_| CoreError::Integrity)?;
            name = table
                .get("logoFile")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| {
                    table
                        .get("mods")
                        .and_then(|v| v.as_array())?
                        .iter()
                        .find_map(|v| v.get("logoFile")?.as_str().map(String::from))
                });
        }
    }
    if name.is_none()
        && let Some(bytes) = entry(&mut zip, "mcmod.info", 524_288)?
    {
        let json: serde_json::Value = serde_json::from_slice(&bytes)?;
        let mods = json.as_array().or_else(|| json.get("modList")?.as_array());
        name = mods.and_then(|items| {
            items
                .iter()
                .find_map(|v| v.get("logoFile")?.as_str().map(String::from))
        });
    }
    let Some(name) = name else {
        return Ok(None);
    };
    let relative = crate::game::fs::relative(&name)?;
    if !relative
        .extension()
        .is_some_and(|v| v.eq_ignore_ascii_case("png"))
    {
        return Ok(None);
    }
    let Some(bytes) = entry(&mut zip, &name, 131_072)? else {
        return Ok(None);
    };
    if bytes.len() < 33 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[8..16] != b"\0\0\0\rIHDR" {
        return Ok(None);
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if width == 0 || height == 0 || width > 1024 || height > 1024 {
        return Ok(None);
    }
    Ok(Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(bytes)
    )))
}
