use crate::{
    commands::AppState,
    error::{CommandError, CoreError},
    packs::{PackManager, model::*},
};
use std::path::Path;

async fn work<T: Send + 'static>(
    manager: PackManager,
    action: impl FnOnce(PackManager) -> Result<T, CoreError> + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(move || action(manager).map_err(CommandError::from))
        .await
        .map_err(|_| CommandError::from(CoreError::Worker))?
}
#[tauri::command]
pub async fn provider_pack_preview(
    project_id: String,
    version_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<PackPreview, CommandError> {
    work(state.packs.clone(), move |m| {
        m.provider_preview(&project_id, &version_id)
    })
    .await
}
#[tauri::command]
pub async fn pack_preview(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<PackPreview, CommandError> {
    work(state.packs.clone(), move |m| m.preview(Path::new(&path))).await
}
#[tauri::command]
pub async fn pick_pack(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<Option<PackPreview>, CommandError> {
    work(state.packs.clone(), move |m| {
        let Some(path) = rfd::FileDialog::new()
            .set_parent(&window)
            .set_title("Sporium — import")
            .add_filter("Sporium / Modrinth", &["sporium", "mrpack"])
            .pick_file()
        else {
            return Ok(None);
        };
        m.preview(&path).map(Some)
    })
    .await
}
#[tauri::command]
pub async fn external_scan(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ExternalCandidate>, CommandError> {
    work(state.packs.clone(), move |m| m.scan(Path::new(&path))).await
}
#[tauri::command]
pub async fn pick_external(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<Option<Vec<ExternalCandidate>>, CommandError> {
    work(state.packs.clone(), move |m| {
        let Some(path) = rfd::FileDialog::new()
            .set_parent(&window)
            .set_title("Sporium — launcher / instance directory")
            .pick_folder()
        else {
            return Ok(None);
        };
        m.scan(&path).map(Some)
    })
    .await
}
#[tauri::command]
pub async fn external_preview(
    key: String,
    state: tauri::State<'_, AppState>,
) -> Result<PackPreview, CommandError> {
    work(state.packs.clone(), move |m| m.external_preview(&key)).await
}
#[tauri::command]
pub async fn pack_import(
    token: String,
    name: String,
    optional: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<PackJob, CommandError> {
    work(state.packs.clone(), move |m| {
        m.start(&token, &name, optional)
    })
    .await
}
#[tauri::command]
pub async fn pack_dismiss(
    token: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    work(state.packs.clone(), move |m| {
        m.dismiss(&token);
        Ok(())
    })
    .await
}
#[tauri::command]
pub fn pack_state(state: tauri::State<'_, AppState>) -> Option<PackJob> {
    state.packs.snapshot()
}
#[tauri::command]
pub fn pack_cancel(state: tauri::State<'_, AppState>) {
    state.packs.cancel();
}
#[tauri::command]
pub fn pack_opening(state: tauri::State<'_, AppState>) -> Vec<String> {
    state.packs.opening()
}
#[tauri::command]
pub async fn pack_export(
    request: PackExportRequest,
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<PackExport, CommandError> {
    work(state.packs.clone(), move |m| {
        m.export(request, Path::new(&path))
    })
    .await
}
#[tauri::command]
pub async fn pick_pack_export(
    request: PackExportRequest,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<Option<PackExport>, CommandError> {
    work(state.packs.clone(), move |m| {
        let Some(path) = rfd::FileDialog::new()
            .set_parent(&window)
            .set_title("Sporium — export")
            .add_filter("Sporium", &["sporium"])
            .set_file_name("pack.sporium")
            .save_file()
        else {
            return Ok(None);
        };
        m.export(request, &path).map(Some)
    })
    .await
}
#[tauri::command]
pub async fn register_pack_formats() -> Result<(), CommandError> {
    tauri::async_runtime::spawn_blocking(register)
        .await
        .map_err(|_| CommandError::from(CoreError::Worker))?
        .map_err(CommandError::from)
}
fn register() -> Result<(), CoreError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let exe = std::env::current_exe()?;
        crate::instances::filesystem::no_links(&exe)?;
        let command = format!("\"{}\" \"%1\"", exe.display());
        // HKCU only. Never claim the user's default for .mrpack or invoke an arbitrary executable.
        for (key, value, data) in [
            ("Software\\Classes\\Sporium.Pack", "", "Sporium pack"),
            (
                "Software\\Classes\\Sporium.Pack\\shell\\open\\command",
                "",
                command.as_str(),
            ),
            (
                "Software\\Classes\\.sporium\\OpenWithProgids",
                "Sporium.Pack",
                "",
            ),
            (
                "Software\\Classes\\.mrpack\\OpenWithProgids",
                "Sporium.Pack",
                "",
            ),
        ] {
            let mut process = std::process::Command::new("reg.exe");
            process.args([
                "add",
                &format!("HKCU\\{key}"),
                if value.is_empty() { "/ve" } else { "/v" },
            ]);
            if !value.is_empty() {
                process.arg(value);
            }
            let status = process
                .args(["/t", "REG_SZ", "/d", data, "/f"])
                .creation_flags(0x08000000)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()?;
            if !status.success() {
                return Err(CoreError::OpenFolderFailed);
            }
        }
        Ok(())
    }
    #[cfg(not(windows))]
    Err(CoreError::ContentUnsupported)
}
