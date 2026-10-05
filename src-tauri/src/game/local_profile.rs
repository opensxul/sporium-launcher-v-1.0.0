use md5::{Digest, Md5};

pub fn default_nickname() -> String {
    "SporiumLocal".into()
}

pub fn valid_nickname(name: &str) -> bool {
    (1..=16).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

pub fn offline_uuid(name: &str) -> uuid::Uuid {
    // Minecraft's local identity convention. This is not an authenticated account UUID.
    let hash: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    uuid::Builder::from_md5_bytes(hash).into_uuid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_identity_is_stable_and_matches_minecraft_convention() {
        assert_eq!(
            offline_uuid("Notch").to_string(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
        assert_eq!(offline_uuid("Notch"), offline_uuid("Notch"));
        assert_ne!(offline_uuid("Notch"), offline_uuid("notch"));
        assert!(!offline_uuid("Notch").is_nil());
    }

    #[test]
    fn nickname_validation_rejects_invalid_local_identities() {
        for name in [
            "",
            "with space",
            "--demo",
            "Алекс",
            "01234567890123456",
            "test\0",
        ] {
            assert!(!valid_nickname(name));
        }
        for name in ["A", "Player_123", "0123456789012345"] {
            assert!(valid_nickname(name));
        }
    }
}
