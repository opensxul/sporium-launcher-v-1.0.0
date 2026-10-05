use crate::content::automatic::{AutomaticPolicy, AutomaticReport};
use crate::{
    commands::AppState,
    content::{ContentManager, model::*},
    error::{CommandError, CoreError},
};
#[tauri::command]
pub async fn automatic_policy(
    state: tauri::State<'_, AppState>,
) -> Result<AutomaticPolicy, CommandError> {
    work(state.content.clone(), |m| m.automatic_policy()).await
}
#[tauri::command]
pub async fn save_automatic_policy(
    value: AutomaticPolicy,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    work(state.content.clone(), move |m| {
        m.save_automatic_policy(value)
    })
    .await
}
#[tauri::command]
pub fn automatic_reports(state: tauri::State<'_, AppState>) -> Vec<AutomaticReport> {
    state.content.automatic_reports()
}
#[tauri::command]
pub async fn automatic_check(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<AutomaticReport, CommandError> {
    let projects = state.projects.clone();
    work(state.content.clone(), move |m| {
        m.automatic_cycle(&id, &projects)
    })
    .await
}
async fn work<T: Send + 'static>(
    manager: ContentManager,
    action: impl FnOnce(ContentManager) -> Result<T, CoreError> + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(move || action(manager).map_err(CommandError::from))
        .await
        .map_err(|_| CommandError::from(CoreError::Worker))?
}
#[tauri::command]
pub async fn content_worlds(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ContentWorld>, CommandError> {
    work(state.content.clone(), move |m| m.worlds(&id)).await
}
#[tauri::command]
pub async fn world_archive_plan(
    request: WorldArchiveRequest,
    state: tauri::State<'_, AppState>,
) -> Result<WorldArchivePlan, CommandError> {
    work(state.content.clone(), move |m| m.world_plan(request)).await
}
#[tauri::command]
pub async fn pick_world_archive(
    mut request: WorldArchiveRequest,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<Option<WorldArchivePlan>, CommandError> {
    work(state.content.clone(), move |m| {
        let Some(file) = rfd::FileDialog::new()
            .set_parent(&window)
            .set_title("Sporium — ZIP")
            .add_filter("Minecraft world / datapack", &["zip"])
            .pick_file()
        else {
            return Ok(None);
        };
        request.source = file.to_string_lossy().to_string();
        m.world_plan(request).map(Some)
    })
    .await
}
#[tauri::command]
pub async fn finish_world_archive(
    token: String,
    accept_unknown: bool,
    cancel: bool,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    work(state.content.clone(), move |m| {
        m.world_finish(&token, accept_unknown, cancel)
    })
    .await
}
#[tauri::command]
pub async fn world_project_plan(
    request: WorldProjectRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ContentPlan, CommandError> {
    work(state.content.clone(), move |m| {
        m.world_project_plan(request)
    })
    .await
}
#[tauri::command]
pub async fn local_content_dependencies(
    token: String,
    state: tauri::State<'_, AppState>,
) -> Result<LocalContentPlan, CommandError> {
    work(state.content.clone(), move |m| m.local_dependencies(&token)).await
}
#[tauri::command]
pub async fn content_dependency_plan(
    id: String,
    files: Vec<ContentSelection>,
    state: tauri::State<'_, AppState>,
) -> Result<LocalDependencyPlan, CommandError> {
    work(state.content.clone(), move |m| {
        m.dependency_plan(&id, files)
    })
    .await
}
#[tauri::command]
pub async fn content_diagnostics(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ModDiagnostics, CommandError> {
    work(state.content.clone(), move |m| m.diagnostics(&id)).await
}
#[tauri::command]
pub async fn content_adoption_plan(
    request: ContentAdoptionRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ContentAdoptionPlan, CommandError> {
    work(state.content.clone(), move |m| m.adoption_plan(request)).await
}
#[tauri::command]
pub async fn finish_content_adoption(
    token: String,
    accept_unknown: bool,
    cancel: bool,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    work(state.content.clone(), move |m| {
        m.adoption_finish(&token, accept_unknown, cancel)
    })
    .await
}
#[tauri::command]
pub async fn local_content_icon(
    id: String,
    directory: String,
    filename: String,
    sha512: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, CommandError> {
    work(state.content.clone(), move |m| {
        m.local_icon(&id, &directory, &filename, sha512.as_deref())
    })
    .await
}
#[tauri::command]
pub async fn content_updates(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ContentUpdate>, CommandError> {
    work(state.content.clone(), move |m| m.updates(&id)).await
}
#[tauri::command]
pub async fn content_restore_points(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ContentRestorePoint>, CommandError> {
    work(state.content.clone(), move |m| m.restore_points(&id)).await
}
#[tauri::command]
pub async fn restore_content(
    id: String,
    point: String,
    settings: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    work(state.content.clone(), move |m| {
        m.restore_content_options(&id, &point, settings.unwrap_or(false))
    })
    .await
}
#[tauri::command]
pub async fn content_update_plan(
    request: ContentUpdateRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ContentUpdatePlan, CommandError> {
    work(state.content.clone(), move |m| m.update_plan(request)).await
}
#[tauri::command]
pub async fn content_update_policy(
    id: String,
    policy: ContentUpdatePolicy,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    work(state.content.clone(), move |m| m.update_policy(&id, policy)).await
}
#[tauri::command]
pub async fn untracked_content(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<UntrackedContent>, CommandError> {
    work(state.content.clone(), move |m| m.untracked(&id)).await
}
#[tauri::command]
pub async fn local_content_plan(
    id: String,
    paths: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<LocalContentPlan, CommandError> {
    work(state.content.clone(), move |m| m.local_plan(&id, paths)).await
}
#[tauri::command]
pub async fn pick_local_content(
    id: String,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<Option<LocalContentPlan>, CommandError> {
    work(state.content.clone(), move |m| {
        let Some(files) = rfd::FileDialog::new()
            .set_parent(&window)
            .set_title("Sporium — JAR")
            .add_filter("Minecraft mods", &["jar"])
            .pick_files()
        else {
            return Ok(None);
        };
        m.local_plan(
            &id,
            files
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        )
        .map(Some)
    })
    .await
}
#[tauri::command]
pub async fn finish_local_content(
    token: String,
    accept_unknown: bool,
    cancel: bool,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    work(state.content.clone(), move |m| {
        m.local_finish(&token, accept_unknown, cancel)
    })
    .await
}
#[tauri::command]
pub async fn content_search(
    query: CatalogQuery,
    state: tauri::State<'_, AppState>,
) -> Result<CatalogPage, CommandError> {
    work(state.content.clone(), move |m| m.search(query)).await
}
#[tauri::command]
pub async fn content_icon(
    project_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, CommandError> {
    work(state.content.clone(), move |m| m.icon(&project_id)).await
}
#[tauri::command]
pub async fn content_tags(state: tauri::State<'_, AppState>) -> Result<ContentTags, CommandError> {
    work(state.content.clone(), |m| m.tags()).await
}
#[tauri::command]
pub async fn content_details(
    project_id: String,
    instance_id: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ContentDetails, CommandError> {
    work(state.content.clone(), move |m| {
        m.details(&project_id, instance_id.as_deref())
    })
    .await
}
#[tauri::command]
pub async fn content_plan(
    request: ContentRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ContentPlan, CommandError> {
    work(state.content.clone(), move |m| m.plan(request)).await
}
#[tauri::command]
pub async fn content_create_plan(
    request: ContentCreateRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ContentPlan, CommandError> {
    let game = state.game.clone();
    work(state.content.clone(), move |m| {
        if !game
            .catalog(false)?
            .versions
            .iter()
            .any(|v| v.id == request.instance.minecraft_version)
        {
            return Err(CoreError::UnsupportedVersion);
        }
        if request.instance.loader != crate::instances::model::Loader::Vanilla
            && crate::game::loaders::catalog(
                game.library.root(),
                request.instance.loader,
                &request.instance.minecraft_version,
            )?
            .versions
            .is_empty()
        {
            return Err(CoreError::ContentIncompatible);
        }
        m.create_plan(request)
    })
    .await
}
#[tauri::command]
pub async fn content_install(
    token: String,
    state: tauri::State<'_, AppState>,
) -> Result<ContentJob, CommandError> {
    let game = state.game.clone();
    work(state.content.clone(), move |m| {
        m.start_with_game(&token, Some(game))
    })
    .await
}
#[tauri::command]
pub fn content_state(state: tauri::State<'_, AppState>) -> Option<ContentJob> {
    state.content.snapshot()
}
#[tauri::command]
pub fn content_cancel(state: tauri::State<'_, AppState>) {
    state.content.cancel();
}
#[tauri::command]
pub async fn installed_content(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<InstalledContent>, CommandError> {
    work(state.content.clone(), move |m| m.installed(&id)).await
}
#[tauri::command]
pub fn open_content_project(id: String) -> Result<(), CommandError> {
    crate::content::modrinth::valid_id(&id)?;
    opener::open(format!("https://modrinth.com/project/{id}"))
        .map_err(|_| CommandError::from(CoreError::OpenFolderFailed))
}
#[tauri::command]
pub async fn change_content(
    request: ContentChange,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    work(state.content.clone(), move |m| m.change(request)).await
}
#[tauri::command]
pub async fn content_history(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ContentHistory>, CommandError> {
    work(state.content.clone(), move |m| m.history(&id)).await
}
