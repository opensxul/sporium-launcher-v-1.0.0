use base64::Engine;
use minisign_verify::{PublicKey, Signature};

#[test]
#[ignore = "Requires freshly built signed releases/SporiumLauncher.exe"]
fn installer_signature_rejects_tampering_and_matches_manifest() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("src-tauri/tauri.conf.json")).unwrap())
            .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("releases/latest.json")).unwrap()).unwrap();
    let decode = |input: &str| {
        String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(input.trim())
                .unwrap(),
        )
        .unwrap()
    };
    let public = PublicKey::decode(&decode(
        config["plugins"]["updater"]["pubkey"].as_str().unwrap(),
    ))
    .unwrap();
    let signature_text =
        std::fs::read_to_string(root.join("releases/SporiumLauncher.exe.sig")).unwrap();
    let decoded = decode(&signature_text);
    let signature = Signature::decode(&decoded).unwrap();
    let mut bytes = std::fs::read(root.join("releases/SporiumLauncher.exe")).unwrap();
    assert!(bytes.starts_with(b"MZ"));
    public.verify(&bytes, &signature, false).unwrap();
    assert_eq!(manifest["version"], config["version"]);
    assert_eq!(manifest["size"].as_u64(), Some(bytes.len() as u64));
    let platform = &manifest["platforms"]["windows-x86_64"];
    assert_eq!(
        platform["signature"].as_str().unwrap(),
        signature_text.trim()
    );
    assert_eq!(
        platform["url"].as_str().unwrap(),
        format!(
            "https://github.com/opensxul/sporium-launcher-v-1.0.0/releases/download/v{}/SporiumLauncher.exe",
            env!("CARGO_PKG_VERSION")
        )
    );
    assert!(decoded.lines().any(|line| {
        line.starts_with("trusted comment:")
            && line
                .split('\t')
                .any(|field| field == format!("version:{}", env!("CARGO_PKG_VERSION")))
    }));
    let end = bytes.len() - 1;
    bytes[end] ^= 1;
    assert!(
        public.verify(&bytes, &signature, false).is_err(),
        "Modified installer must be rejected"
    );
}
