use crate::{
    commands::AppState,
    error::{CommandError, CoreError},
    projects::{ProjectManager, model::*},
};
#[tauri::command]
pub async fn project_replan(
    token: String,
    groups: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectPlan, CommandError> {
    work(state.projects.clone(), move |m| m.replan(&token, &groups)).await
}
async fn work<T: Send + 'static>(
    manager: ProjectManager,
    action: impl FnOnce(ProjectManager) -> Result<T, CoreError> + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(move || action(manager).map_err(CommandError::from))
        .await
        .map_err(|_| CommandError::from(CoreError::Worker))?
}
#[tauri::command]
pub async fn project_view(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectView, CommandError> {
    work(state.projects.clone(), move |m| m.view(&id)).await
}
#[tauri::command]
pub async fn studio_files(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ProjectFile>, CommandError> {
    work(state.projects.clone(), move |m| m.studio_files(&id)).await
}
#[tauri::command]
pub async fn studio_plan(
    request: StudioRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectPlan, CommandError> {
    work(state.projects.clone(), move |m| m.studio_plan(request)).await
}
#[tauri::command]
pub async fn project_check(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectUpdate, CommandError> {
    work(state.projects.clone(), move |m| m.check(&id)).await
}
#[tauri::command]
pub async fn project_repair_plan(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectPlan, CommandError> {
    work(state.projects.clone(), move |m| m.repair_plan(&id)).await
}
#[tauri::command]
pub async fn project_pack_plan(
    id: String,
    version_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectPlan, CommandError> {
    work(state.projects.clone(), move |m| {
        m.pack_plan(&id, &version_id)
    })
    .await
}
#[tauri::command]
pub async fn project_source_plan(
    id: String,
    groups: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectPlan, CommandError> {
    work(state.projects.clone(), move |m| m.source_plan(&id, &groups)).await
}
#[tauri::command]
pub async fn project_pick_manifest(
    id: String,
    groups: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<Option<ProjectPlan>, CommandError> {
    let file = rfd::AsyncFileDialog::new()
        .add_filter("Sporium project manifest", &["json"])
        .pick_file()
        .await;
    let Some(file) = file else { return Ok(None) };
    let file = file.path().to_path_buf();
    work(state.projects.clone(), move |m| {
        m.local_plan(&id, &file, &groups).map(Some)
    })
    .await
}
#[tauri::command]
pub async fn project_export(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, CommandError> {
    let file = rfd::AsyncFileDialog::new()
        .add_filter("Sporium project manifest", &["json"])
        .set_file_name("project.json")
        .save_file()
        .await;
    let Some(file) = file else { return Ok(false) };
    let file = file.path().to_path_buf();
    work(state.projects.clone(), move |m| {
        m.export(&id, &file).map(|_| true)
    })
    .await
}
#[tauri::command]
pub async fn project_apply(
    token: String,
    accept_changes: bool,
    state: tauri::State<'_, AppState>,
) -> Result<String, CommandError> {
    work(state.projects.clone(), move |m| {
        m.apply(&token, accept_changes)
    })
    .await
}
#[tauri::command]
pub fn project_dismiss(token: String, state: tauri::State<'_, AppState>) {
    state.projects.dismiss(&token);
}
#[tauri::command]
pub fn project_cancel(state: tauri::State<'_, AppState>) {
    state.projects.cancel();
}
