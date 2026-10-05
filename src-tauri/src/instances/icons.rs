use super::model::valid_id;
use super::{Library, filesystem::no_links, model::*, repository};
use crate::error::CoreError;
use crate::game::{
    fs::{read_limited, write_atomic},
    network::hex,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use sha2::Digest;
use std::{
    io::{Cursor, Read},
    path::Path,
};
use ts_rs::TS;

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InstanceLogo {
    pub id: String,
    pub name: String,
    pub image: String,
}

// Artwork enters this catalog only after the user supplies the final approved PNG batch.
pub fn catalog() -> Vec<InstanceLogo> {
    vec![]
}

fn thumbnail(bytes: &[u8]) -> Result<Vec<u8>, CoreError> {
    if bytes.len() > 8_000_000 {
        return Err(CoreError::InvalidInput);
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
    ) {
        return Err(CoreError::InvalidInput);
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64_000_000);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|_| CoreError::InvalidInput)?;
    let resized = decoded.resize(256, 256, image::imageops::FilterType::Lanczos3);
    let mut output = Cursor::new(Vec::new());
    resized
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|_| CoreError::InvalidInput)?;
    let bytes = output.into_inner();
    if bytes.len() > 500_000 {
        return Err(CoreError::InvalidInput);
    }
    Ok(bytes)
}

impl Library {
    pub fn custom_icon(
        &self,
        request: RecordRequest,
        source: &Path,
    ) -> Result<LibraryChange, CoreError> {
        no_links(source)?;
        if !source.is_absolute() || !source.is_file() || source.metadata()?.len() > 8_000_000 {
            return Err(CoreError::InvalidInput);
        }
        let mut input = vec![];
        std::fs::File::open(source)?
            .take(8_000_001)
            .read_to_end(&mut input)?;
        let png = thumbnail(&input)?;
        let hash = hex(&sha2::Sha256::digest(&png));
        self.access(|db, paths| {
            let mut instance = repository::instance(db, &request.id)?;
            repository::revision(instance.revision, request.expected_revision)?;
            write_atomic(
                paths,
                &paths
                    .root()
                    .join("shared/instance-icons")
                    .join(format!("{hash}.png")),
                &png,
            )?;
            instance.icon_ref = Some(format!("custom:{hash}"));
            instance.icon_source = IconSource::Custom;
            instance.revision = next_revision(instance.revision)?;
            instance.updated_at = now();
            repository::update_instance(db, &instance)?;
            repository::change(db, instance.id, None)
        })
    }
    pub fn select_icon(
        &self,
        request: RecordRequest,
        choice: &str,
    ) -> Result<LibraryChange, CoreError> {
        self.access(|db, _| {
            let mut instance = repository::instance(db, &request.id)?;
            repository::revision(instance.revision, request.expected_revision)?;
            let logos = catalog();
            let selected = if choice == "automatic" {
                None
            } else if choice == "random" {
                if logos.is_empty() {
                    return Err(CoreError::ContentUnsupported);
                }
                Some(&logos[(uuid::Uuid::new_v4().as_u128() % logos.len() as u128) as usize])
            } else {
                Some(
                    logos
                        .iter()
                        .find(|logo| logo.id == choice)
                        .ok_or(CoreError::InvalidInput)?,
                )
            };
            instance.icon_ref = selected.map(|logo| format!("builtin:{}", logo.id));
            instance.icon_source = if selected.is_some() {
                IconSource::Builtin
            } else {
                IconSource::Automatic
            };
            if selected.is_none() {
                instance.icon_ref = automatic_icon(&instance.id)?;
            }
            instance.revision = next_revision(instance.revision)?;
            instance.updated_at = now();
            repository::update_instance(db, &instance)?;
            repository::change(db, instance.id, None)
        })
    }
    pub fn icon_image(&self, id: &str) -> Result<Option<String>, CoreError> {
        self.access(|db, paths| {
            let instance = repository::instance(db, id)?;
            let Some(reference) = instance.icon_ref else {
                return Ok(None);
            };
            if let Some(hash) = reference.strip_prefix("custom:") {
                if !crate::packs::archive::hash_valid(hash, 64) {
                    return Err(CoreError::Integrity);
                }
                let bytes = read_limited(
                    paths,
                    &paths
                        .root()
                        .join("shared/instance-icons")
                        .join(format!("{hash}.png")),
                    500_000,
                )?;
                if hex(&sha2::Sha256::digest(&bytes)) != hash {
                    return Err(CoreError::Integrity);
                }
                return Ok(Some(format!(
                    "data:image/png;base64,{}",
                    STANDARD.encode(bytes)
                )));
            }
            if let Some(id) = reference.strip_prefix("builtin:") {
                return Ok(catalog()
                    .into_iter()
                    .find(|logo| logo.id == id)
                    .map(|logo| logo.image));
            }
            Ok(None)
        })
    }
}

// Populate only with approved, user-supplied assets at the phase 14 asset gate.
pub const APPROVED_BUILTINS: &[&str] = &[];

/// A UUID v4 provides random assignment that remains stable across restarts.
pub fn automatic_icon(id: &str) -> Result<Option<String>, CoreError> {
    choose(id, APPROVED_BUILTINS)
}

fn choose(id: &str, catalog: &[&str]) -> Result<Option<String>, CoreError> {
    valid_id(id)?;
    if catalog.is_empty() {
        return Ok(None);
    }
    let value = uuid::Uuid::parse_str(id).map_err(|_| CoreError::InvalidInput)?;
    let index = value.as_u128() % catalog.len() as u128;
    Ok(Some(catalog[index as usize].to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thumbnails_decode_supported_formats_preserve_ratio_and_bound_dimensions() {
        for format in [
            image::ImageFormat::Png,
            image::ImageFormat::Jpeg,
            image::ImageFormat::WebP,
        ] {
            let original = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                800,
                400,
                image::Rgb([31, 160, 50]),
            ));
            let mut encoded = Cursor::new(Vec::new());
            original.write_to(&mut encoded, format).unwrap();
            let result = thumbnail(encoded.get_ref()).unwrap();
            assert_eq!(
                image::guess_format(&result).unwrap(),
                image::ImageFormat::Png
            );
            let decoded = image::load_from_memory(&result).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (256, 128));
        }
    }
    #[test]
    fn icons_reject_corruption_svg_oversized_files_and_decompression_dimensions() {
        assert!(thumbnail(b"<svg><script>alert(1)</script></svg>").is_err());
        assert!(thumbnail(b"\x89PNG\r\n\x1a\ntruncated").is_err());
        assert!(thumbnail(&vec![0; 8_000_001]).is_err());
        let large = image::DynamicImage::new_rgba8(4097, 1);
        let mut encoded = Cursor::new(Vec::new());
        large
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        assert!(thumbnail(encoded.get_ref()).is_err());
    }
    #[test]
    fn assignment_is_stable_and_uses_only_approved_catalog() {
        let id = "c49c5d43-ab37-4c2f-a19a-67e78ed560a0";
        assert_eq!(automatic_icon(id).unwrap(), None);
        let catalog = ["fixture-one", "fixture-two"];
        let selected = choose(id, &catalog).unwrap().unwrap();
        assert!(catalog.contains(&selected.as_str()));
        assert_eq!(
            choose(id, &catalog).unwrap().as_deref(),
            Some(selected.as_str())
        );
        assert!(choose("../../bad", &catalog).is_err());
    }
}
