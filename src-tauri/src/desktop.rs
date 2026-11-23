use crate::{
    commands::AppState,
    error::{CommandError, CoreError},
    world::AfterLaunch,
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use windows_registry::CURRENT_USER;

const OPTIONS: &str = r"Software\Sporium\Launcher";
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
fn option(name: &str) -> Option<String> {
    CURRENT_USER.open(OPTIONS).ok()?.get_string(name).ok()
}
fn write_option(name: &str, value: &str) -> Result<(), CommandError> {
    CURRENT_USER
        .create(OPTIONS)
        .and_then(|k| k.set_string(name, value))
        .map_err(|_| CoreError::InvalidSettings.into())
}
fn safe_library(path: &Path) -> Result<(), CoreError> {
    if !path.is_absolute() || path.parent().is_none() || !path.is_dir() {
        return Err(CoreError::UnsafePath);
    }
    crate::instances::filesystem::no_links(path)?;
    let db = path.join("launcher/sporium.sqlite3");
    crate::instances::filesystem::no_links(&db)?;
    if !db.is_file() && std::fs::read_dir(path)?.next().is_some() {
        return Err(CoreError::InvalidSettings);
    }
    Ok(())
}
pub fn library_root(default: PathBuf) -> Result<PathBuf, CoreError> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() == Some(std::ffi::OsStr::new("--library")) {
        let path = PathBuf::from(args.next().ok_or(CoreError::InvalidSettings)?);
        safe_library(&path)?;
        return Ok(path);
    }
    Ok(option("Library")
        .map(PathBuf::from)
        .filter(|p| safe_library(p).is_ok())
        .unwrap_or(default))
}
fn restore(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
pub fn install_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "restore", "Sporium Launcher", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open])?;
    let mut tray = TrayIconBuilder::with_id("sporium")
        .tooltip("Sporium Launcher")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id.as_ref() == "restore" {
                restore(app)
            }
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                restore(tray.app_handle())
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
pub fn create_window(app: &tauri::App) -> tauri::Result<()> {
    let mut builder = tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?;
    if option("SoftwareRendering").as_deref() == Some("1") {
        builder = builder.additional_browser_args("--disable-gpu");
    }
    builder.build()?;
    Ok(())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPreferences {
    startup: bool,
    acceleration: bool,
    library: Option<String>,
}
#[tauri::command]
pub fn desktop_preferences() -> DesktopPreferences {
    DesktopPreferences {
        startup: CURRENT_USER
            .open(RUN)
            .is_ok_and(|key| key.get_string("SporiumLauncher").is_ok()),
        acceleration: option("SoftwareRendering").as_deref() != Some("1"),
        library: option("Library").filter(|s| !s.is_empty()),
    }
}
#[tauri::command]
pub fn set_startup(enabled: bool) -> Result<(), CommandError> {
    let key = CURRENT_USER
        .create(RUN)
        .map_err(|_| CoreError::InvalidSettings)?;
    if enabled {
        let exe = std::env::current_exe().map_err(CoreError::Io)?;
        key.set_string("SporiumLauncher", format!("\"{}\"", exe.display()))
            .map_err(|_| CoreError::InvalidSettings)?;
    } else if key.get_string("SporiumLauncher").is_ok() {
        key.remove_value("SporiumLauncher")
            .map_err(|_| CoreError::InvalidSettings)?;
    }
    Ok(())
}
#[tauri::command]
pub fn set_acceleration(enabled: bool) -> Result<(), CommandError> {
    write_option("SoftwareRendering", if enabled { "0" } else { "1" })
}
#[tauri::command]
pub async fn select_library() -> Result<Option<String>, CommandError> {
    let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await else {
        return Ok(None);
    };
    let path = folder.path();
    safe_library(path)?;
    let path = path.to_str().ok_or(CoreError::InvalidSettings)?;
    write_option("Library", path)?;
    Ok(Some(path.into()))
}
#[tauri::command]
pub fn reset_library() -> Result<(), CommandError> {
    write_option("Library", "")
}
#[tauri::command]
pub fn hide_to_tray(window: tauri::WebviewWindow) -> Result<(), CommandError> {
    window.hide().map_err(|_| CoreError::Worker.into())
}
#[tauri::command]
pub async fn window_after_launch(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    if !state.game.snapshot().sessions.iter().any(|s| s.running) {
        return Ok(());
    }
    let db = state.database.clone();
    let behavior = tauri::async_runtime::spawn_blocking(move || db.load_settings())
        .await
        .map_err(|_| CoreError::Worker)??
        .values
        .world
        .after_launch;
    match behavior {
        AfterLaunch::Open => Ok(()),
        AfterLaunch::Minimize => window.minimize(),
        AfterLaunch::Tray => window.hide(),
    }
    .map_err(|_| CoreError::Worker.into())
}
