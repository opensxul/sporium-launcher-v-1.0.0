use serde_json::json;
use sporium_lib::{
    error::CoreError,
    game::{
        fs::relative,
        network::{Hash, approved, verify},
        rules::{Environment, allowed, arguments},
    },
    instances::model::game_version,
};
use std::fs;

#[test]
fn official_legacy_identifiers_are_display_metadata_not_paths() {
    assert_eq!(
        game_version("1.14.2 Pre-Release 4").unwrap(),
        "1.14.2 Pre-Release 4"
    );
    assert_eq!(
        game_version("3D Shareware v1.34").unwrap(),
        "3D Shareware v1.34"
    );
    assert!(game_version("../../escape").is_err());
}

#[test]
fn download_targets_reject_traversal_windows_devices_and_ads() {
    for path in [
        "../outside",
        "C:/outside",
        "/absolute",
        "a/../../x",
        "a\\..\\x",
        "CON.jar",
        "a/NUL",
        "a/COM1.dll",
        "file:stream",
        "file. ",
        "file?x",
        "",
    ] {
        assert!(relative(path).is_err(), "{path}");
    }
    assert!(relative("org/lwjgl/native/file.dll").is_ok());
}

#[test]
fn requests_and_redirects_stay_on_approved_https_services() {
    for url in [
        "http://libraries.minecraft.net/file.jar",
        "https://evil.example/file",
        "https://github.com/other/project/releases/download/x.zip",
        "https://user@piston-meta.mojang.com/x",
        "https://piston-meta.mojang.com:444/x",
        "https://127.0.0.1/x",
        "file:///C:/secret",
    ] {
        assert!(!approved(&reqwest::Url::parse(url).unwrap()), "{url}");
    }
    assert!(approved(
        &reqwest::Url::parse(
            "https://github.com/adoptium/temurin25-binaries/releases/download/test/runtime.zip"
        )
        .unwrap()
    ));
    assert!(approved(
        &reqwest::Url::parse("https://resources.download.minecraft.net/ab/abcdef").unwrap()
    ));
}

#[test]
fn corrupt_or_truncated_artifacts_are_never_treated_as_installed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("payload");
    fs::write(&path, b"abc").unwrap();
    let hash = Hash::Sha1("a9993e364706816aba3e25717850c26c9cd0d89d".into());
    assert!(verify(&path, &hash, 3).unwrap());
    assert!(!verify(&path, &hash, 4).unwrap());
    fs::write(&path, b"abd").unwrap();
    assert!(!verify(&path, &hash, 3).unwrap());
    assert!(matches!(
        verify(&path, &Hash::Sha1("wrong".into()), 3),
        Err(CoreError::Integrity)
    ));
}

#[test]
fn rules_obey_order_architecture_features_and_windows_version_ranges() {
    let env = Environment {
        os: "windows".into(),
        arch: "x86_64".into(),
        version: "10.0.22631".into(),
        demo: true,
    };
    assert!(allowed(None, &env).unwrap());
    assert!(
        !allowed(
            Some(&json!([{"action":"allow","os":{"name":"linux"}}])),
            &env
        )
        .unwrap()
    );
    assert!(
        allowed(
            Some(&json!([{"action":"allow"},{"action":"disallow","os":{"arch":"x86"}}])),
            &env
        )
        .unwrap()
    );
    assert!(
        allowed(
            Some(&json!([{"action":"allow","os":{"versionRange":{"min":"10.0.17134"}}}])),
            &env
        )
        .unwrap()
    );
    assert!(
        !allowed(
            Some(&json!([{"action":"allow","os":{"versionRange":{"max":"10.0.17134"}}}])),
            &env
        )
        .unwrap()
    );
    let args = arguments(&json!(["--username","${auth_player_name}",{"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"},{"rules":[{"action":"allow","features":{"is_quick_play_realms":true}}],"value":["--quickPlayRealms","secret"]}]), &env).unwrap();
    assert_eq!(args, ["--username", "${auth_player_name}", "--demo"]);
}

#[test]
fn version_visibility_defaults_preserve_old_settings() {
    let value: sporium_lib::settings::Settings = serde_json::from_value(
        json!({"schemaVersion":1,"locale":"ru-RU","motion":"system","uiScale":100}),
    )
    .unwrap();
    assert!(value.version_visibility.releases);
    assert!(
        !value.version_visibility.snapshots
            && !value.version_visibility.beta
            && !value.version_visibility.alpha
    );
    assert!(value.custom_java_path.is_none());
}
