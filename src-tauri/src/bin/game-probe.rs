use sporium_lib::{
    game::{GameManager, model::*},
    instances::{Library, model::*},
    storage::Database,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_ansi(false)
        .init();
    // Explicit developer smoke command. Never runs as part of normal tests or app startup.
    let args: Vec<String> = std::env::args().collect();
    if !(3..=6).contains(&args.len()) {
        return Err("Usage: game-probe <absolute-test-root> <official-version-id> [vanilla|fabric|forge|neoforge] [loader-version]".into());
    }
    let root = PathBuf::from(&args[1]);
    if !root.is_absolute() {
        return Err("Test root must be absolute".into());
    }
    let database = Database::new(root.join("launcher/sporium.sqlite3"));
    let library = Library::new(root.clone(), database.clone());
    if let Some(nickname) = args.get(5) {
        let state = sporium_lib::profiles::snapshot(&database)?;
        sporium_lib::profiles::edit(
            &database,
            sporium_lib::profiles::EditProfile {
                action: sporium_lib::profiles::ProfileAction::Rename,
                id: Some(state.active_profile_id),
                nickname: Some(nickname.clone()),
                expected_revision: state.revision,
            },
        )?;
    }
    if let Some(nickname) = args.get(5) {
        let state = sporium_lib::profiles::snapshot(&database)?;
        sporium_lib::profiles::edit(
            &database,
            sporium_lib::profiles::EditProfile {
                action: sporium_lib::profiles::ProfileAction::Rename,
                id: Some(state.active_profile_id),
                nickname: Some(nickname.clone()),
                expected_revision: state.revision,
            },
        )?;
    }
    let game = GameManager::new(library.clone(), database);
    let version = &args[2];
    let loader = match args.get(3).map(String::as_str).unwrap_or("vanilla") {
        "vanilla" => Loader::Vanilla,
        "fabric" => Loader::Fabric,
        "forge" => Loader::Forge,
        "neoforge" => Loader::NeoForge,
        _ => return Err("Unknown loader".into()),
    };
    let snapshot = library.snapshot()?;
    let instance = if let Some(instance) = snapshot
        .instances
        .into_iter()
        .find(|instance| instance.minecraft_version == *version && instance.loader == loader)
    {
        instance
    } else {
        let created = library.create(CreateInstance {
            name: format!(
                "Launch test {} {version}",
                sporium_lib::game::loaders::key(loader)
            ),
            minecraft_version: version.clone(),
            loader,
            collection_id: None,
        })?;
        created
            .snapshot
            .instances
            .into_iter()
            .find(|instance| instance.id == created.affected_id)
            .ok_or("create failed")?
    };
    if let Some(loader_version) = args.get(4) {
        library.configure_launch(ConfigureLaunch {
            id: instance.id.clone(),
            expected_revision: instance.revision,
            loader_version: Some(loader_version.clone()),
        })?;
    }
    let action = GameAction::Local;
    game.start(GameRequest {
        id: instance.id.clone(),
        action,
    })?;
    let start = Instant::now();
    let mut launched = None;
    let mut last_print = Instant::now() - Duration::from_secs(5);
    loop {
        let state = game.snapshot();
        if last_print.elapsed() >= Duration::from_secs(10) {
            println!("{}", serde_json::to_string(&state)?);
            last_print = Instant::now();
        }
        if matches!(
            state.job.as_ref().map(|job| job.phase),
            Some(JobPhase::Failed | JobPhase::Cancelled)
        ) {
            return Err(format!("operation failed: {}", serde_json::to_string(&state)?).into());
        }
        if let Some(session) = state
            .sessions
            .iter()
            .find(|session| session.instance_id == instance.id)
        {
            if !session.running {
                return Err(
                    format!("game exited early: {}", serde_json::to_string(session)?).into(),
                );
            }
            let launched_at = launched.get_or_insert_with(Instant::now);
            if launched_at.elapsed() >= Duration::from_secs(35) {
                let log = std::fs::read_to_string(&session.log_path).unwrap_or_default();
                let renderer_ready = log.contains("OpenAL initialized")
                    && !log.contains("Unable to launch")
                    && !log.contains("Could not create the Java Virtual Machine");
                println!(
                    "PROCESS_ALIVE_35S version={version} log_bytes={} log={}",
                    log.len(),
                    session.log_path
                );
                game.stop(&instance.id)?;
                if !renderer_ready {
                    return Err(format!("Game stayed alive but renderer/audio initialization was not confirmed; inspect {}", session.log_path).into());
                }
                let record = serde_json::json!({ "version":version, "instanceId":instance.id, "aliveSeconds":35, "logPath":session.log_path, "mode":action, "audioInitialized":true, "joinedWorld":log.contains("joined the game") });
                std::fs::create_dir_all(root.join("results"))?;
                std::fs::write(
                    root.join("results").join(if loader == Loader::Vanilla {
                        format!("{version}.json")
                    } else {
                        format!("{}-{version}.json", sporium_lib::game::loaders::key(loader))
                    }),
                    serde_json::to_vec_pretty(&record)?,
                )?;
                break;
            }
        }
        if start.elapsed() > Duration::from_secs(1200) {
            game.cancel();
            return Err("installation timeout".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}
