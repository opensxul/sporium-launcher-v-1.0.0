use super::{
    fs::java_path,
    local_profile::{offline_uuid, valid_nickname},
    metadata::string,
    model::JavaRuntime,
    rules::{self, Environment},
};
use crate::{error::CoreError, instances::filesystem::Paths};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    process::{Child, Stdio},
};

pub struct ResolvedLaunchPlan {
    pub java: PathBuf,
    pub classpath: Vec<PathBuf>,
    pub natives: PathBuf,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub game_dir: PathBuf,
    pub assets: PathBuf,
    pub main_class: String,
    pub logging: Option<PathBuf>,
    pub memory_mb: u64,
}

pub struct LaunchInputs<'a> {
    pub paths: &'a Paths,
    pub metadata: &'a Value,
    pub runtime: &'a JavaRuntime,
    pub classpath: Vec<PathBuf>,
    pub natives: PathBuf,
    pub game_dir: PathBuf,
    pub game_assets: PathBuf,
    pub asset_id: String,
    pub logging: Option<PathBuf>,
    pub nickname: &'a str,
    pub default_memory_mb: u32,
    pub(crate) identity: Option<&'a crate::microsoft::Identity>,
}

pub fn resolve(input: LaunchInputs<'_>) -> Result<ResolvedLaunchPlan, CoreError> {
    let LaunchInputs {
        paths,
        metadata,
        runtime,
        classpath,
        natives,
        game_dir,
        game_assets,
        asset_id,
        logging,
        nickname,
        default_memory_mb,
        identity,
    } = input;
    let legacy = matches!(
        metadata.get("type").and_then(Value::as_str),
        Some("old_beta" | "old_alpha")
    );
    if !valid_nickname(nickname) {
        return Err(CoreError::InvalidInput);
    }
    let env = Environment::windows(false);
    let asset_root = paths.root().join("shared/assets");
    let library_root = paths.root().join("shared/libraries");
    let classpath_text = classpath
        .iter()
        .map(|path| java_path(path))
        .collect::<Vec<_>>()
        .join(";");
    let mut values: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", nickname.to_owned()),
        ("auth_uuid", offline_uuid(nickname).simple().to_string()),
        ("auth_access_token", "0".into()),
        ("auth_session", "0".into()),
        ("user_type", "legacy".into()),
        ("user_properties", "{}".into()),
        ("auth_xuid", "".into()),
        ("clientid", "".into()),
        ("version_name", string(metadata, "id")?.into()),
        ("version_type", string(metadata, "type")?.into()),
        ("game_directory", java_path(&game_dir)),
        ("assets_root", java_path(&asset_root)),
        ("assets_index_name", asset_id),
        ("game_assets", java_path(&game_assets)),
        ("natives_directory", java_path(&natives)),
        ("library_directory", java_path(&library_root)),
        ("classpath_separator", ";".into()),
        ("classpath", classpath_text),
        ("launcher_name", "Sporium".into()),
        ("launcher_version", env!("CARGO_PKG_VERSION").into()),
        ("resolution_width", "854".into()),
        ("resolution_height", "480".into()),
    ]);
    if let Some(auth) = identity {
        for (key, value) in [
            ("auth_player_name", &auth.name),
            ("auth_uuid", &auth.uuid),
            ("auth_access_token", &auth.access),
            ("clientid", &auth.client),
            ("auth_xuid", &auth.xuid),
        ] {
            values.insert(key, value.clone());
        }
        values.insert("user_type", "msa".to_owned());
        values.insert(
            "auth_session",
            format!("token:{}:{}", auth.access, auth.uuid),
        );
    }
    if let Some(logging) = &logging {
        values.insert("path", java_path(logging));
    }
    let replace = |text: &str| -> Result<String, CoreError> {
        let mut result = text.to_owned();
        for (key, value) in &values {
            result = result.replace(&format!("${{{key}}}"), value);
        }
        if result.contains("${") || result.contains('\0') {
            return Err(CoreError::UnsupportedVersion);
        }
        Ok(result)
    };
    let system = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing().with_memory(sysinfo::MemoryRefreshKind::everything()),
    );
    let available_mb = system.available_memory() / (1024 * 1024);
    let default_memory = (available_mb / 2).clamp(1024, if legacy { 2048 } else { 4096 });
    let memory_mb = crate::projects::manifest(paths, &game_dir)?
        .and_then(|project| project.launch.memory_mib)
        .or((default_memory_mb != 0).then_some(default_memory_mb))
        .map(u64::from)
        .map(|memory| {
            memory
                .min((available_mb / 2).max(512))
                .min(if legacy { 2048 } else { 32768 })
                .max(512)
        })
        .unwrap_or(default_memory);
    let mut jvm_args = vec![
        format!("-Xmx{memory_mb}M"),
        "-Xms512M".into(),
        "-Dfile.encoding=UTF-8".into(),
    ];
    if let Some(arguments) = metadata.pointer("/arguments/jvm") {
        for argument in rules::arguments(arguments, &env)? {
            let mut resolved = replace(&argument)?;
            // BootstrapLauncher must ignore the original client: Forge supplies its patched
            // Minecraft module separately. Our content-addressed filename differs from the
            // official launcher's ${version_name}.jar assumed by the installer profile.
            if resolved.starts_with("-DignoreList=")
                && let Some(sha) = metadata
                    .pointer("/downloads/client/sha1")
                    .and_then(Value::as_str)
            {
                resolved.push_str(&format!(",{sha}.jar"));
            }
            jvm_args.push(resolved);
        }
    } else {
        for argument in [
            "-Djava.library.path=${natives_directory}",
            "-cp",
            "${classpath}",
        ] {
            jvm_args.push(replace(argument)?);
        }
    }
    if let Some(logging_arg) = metadata
        .pointer("/logging/client/argument")
        .and_then(Value::as_str)
    {
        jvm_args.push(replace(logging_arg)?);
    }
    let raw_args = if let Some(arguments) = metadata.pointer("/arguments/game") {
        rules::arguments(arguments, &env)?
    } else {
        string(metadata, "minecraftArguments")?
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    };
    let game_args = raw_args
        .iter()
        .map(|value| replace(value))
        .collect::<Result<Vec<_>, _>>()?;
    let main_class = string(metadata, "mainClass")?.to_owned();
    if !main_class
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._$".contains(&byte))
    {
        return Err(CoreError::UnsupportedVersion);
    }
    Ok(ResolvedLaunchPlan {
        java: runtime.executable.clone().into(),
        classpath,
        natives,
        jvm_args,
        game_args,
        game_dir,
        assets: asset_root,
        main_class,
        logging,
        memory_mb,
    })
}

pub fn connect_to_server(
    args: &mut Vec<String>,
    address: &str,
    version: &str,
) -> Result<(), CoreError> {
    let (host, port) = crate::world::endpoint(address).ok_or(CoreError::InvalidSettings)?;
    let parts: Vec<u32> = version
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|_| CoreError::UnsupportedVersion)?;
    if parts.as_slice() >= &[1, 20][..] {
        args.extend(["--quickPlayMultiplayer".to_owned(), address.to_owned()]);
    } else if parts.as_slice() >= &[1, 6][..] {
        args.extend([
            "--server".to_owned(),
            host,
            "--port".to_owned(),
            port.to_string(),
        ]);
    } else {
        return Err(CoreError::UnsupportedVersion);
    }
    Ok(())
}

pub fn spawn(plan: &ResolvedLaunchPlan, log: &Path) -> Result<Child, CoreError> {
    let output = File::create(log)?;
    super::java::command(&plan.java)
        .args(&plan.jvm_args)
        .arg(&plan.main_class)
        .args(&plan.game_args)
        .current_dir(&plan.game_dir)
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output)
        .spawn()
        .map_err(|_| CoreError::LaunchFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn join_arguments_match_version_and_validated_endpoint() {
        let mut modern = Vec::new();
        connect_to_server(&mut modern, "play.example:25570", "1.21.1").unwrap();
        assert_eq!(modern, ["--quickPlayMultiplayer", "play.example:25570"]);
        let mut legacy = Vec::new();
        connect_to_server(&mut legacy, "play.example", "1.19.4").unwrap();
        assert_eq!(legacy, ["--server", "play.example", "--port", "25565"]);
        assert!(connect_to_server(&mut vec![], "play.example", "1.5.2").is_err());
        assert!(connect_to_server(&mut vec![], "bad address", "1.21").is_err());
    }

    #[test]
    fn local_plan_preserves_paths_and_skips_demo_features() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(&dir.path().join("with spaces")).unwrap();
        let runtime = JavaRuntime {
            executable: "C:\\Java Runtime\\bin\\java.exe".into(),
            major: 25,
            version: "25".into(),
            architecture: "amd64".into(),
            managed: true,
        };
        let metadata = json!({"id":"26.3", "type":"release", "mainClass":"net.minecraft.client.main.Main",
            "arguments":{"jvm":["-cp","${classpath}"], "game":["--gameDir","${game_directory}","--username","${auth_player_name}","--uuid","${auth_uuid}","--accessToken","${auth_access_token}",{"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"}]}});
        let game_dir = paths.root().join("instance with spaces");
        let classpath = vec![paths.root().join("client with spaces.jar")];
        let resolve_local = |metadata: &Value, nickname| {
            resolve(LaunchInputs {
                paths: &paths,
                metadata,
                runtime: &runtime,
                classpath: classpath.clone(),
                natives: paths.root().join("natives"),
                game_dir: game_dir.clone(),
                game_assets: paths.root().join("assets"),
                asset_id: "34".into(),
                logging: None,
                nickname,
                default_memory_mb: 0,
                identity: None,
            })
        };
        let plan = resolve_local(&metadata, "Local_123").unwrap();
        let mut forge = metadata.clone();
        forge["arguments"]["jvm"] = json!([
            "-DignoreList=client-extra,${version_name}.jar",
            "-cp",
            "${classpath}"
        ]);
        forge["downloads"] = json!({"client":{"sha1":"0123456789012345678901234567890123456789"}});
        let forge_plan = resolve_local(&forge, "Local_123").unwrap();
        assert!(forge_plan.jvm_args.iter().any(|arg| arg
            == "-DignoreList=client-extra,26.3.jar,0123456789012345678901234567890123456789.jar"));
        assert_eq!(
            plan.game_args,
            [
                "--gameDir",
                &java_path(&game_dir),
                "--username",
                "Local_123",
                "--uuid",
                &offline_uuid("Local_123").simple().to_string(),
                "--accessToken",
                "0"
            ]
        );
        assert_eq!(plan.jvm_args.last().unwrap(), &java_path(&classpath[0]));
        assert!(matches!(
            resolve_local(&metadata, "with spaces"),
            Err(CoreError::InvalidInput)
        ));
        let legacy = json!({"id":"b1.7.3", "type":"old_beta", "mainClass":"net.minecraft.launchwrapper.Launch", "minecraftArguments":"${auth_player_name} ${auth_session} --gameDir ${game_directory}"});
        let plan = resolve_local(&legacy, "Local_123").unwrap();
        assert_eq!(
            plan.game_args,
            ["Local_123", "0", "--gameDir", &java_path(&game_dir)]
        );
        let old_release = json!({"id":"1.5.2", "type":"release", "mainClass":"net.minecraft.client.Minecraft", "minecraftArguments":"--username ${auth_player_name} --session ${auth_session}"});
        let plan = resolve_local(&old_release, "Local_123").unwrap();
        assert_eq!(
            plan.game_args,
            ["--username", "Local_123", "--session", "0"]
        );
    }
}
