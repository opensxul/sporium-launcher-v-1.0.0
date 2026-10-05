use crate::{
    commands::AppState,
    error::{CommandError, CoreError},
    instances::model::{ConfigureLaunch, LibraryChange},
    profiles::{EditProfile, ProfileSnapshot},
    skins::SkinView,
};

#[tauri::command]
pub async fn edit_profile(
    request: EditProfile,
    state: tauri::State<'_, AppState>,
) -> Result<ProfileSnapshot, CommandError> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::profiles::edit(&db, request).map_err(CommandError::from)
    })
    .await
    .map_err(|_| CommandError::from(CoreError::Worker))?
}
#[tauri::command]
pub async fn profile_skin(
    nickname: String,
    refresh: bool,
    state: tauri::State<'_, AppState>,
) -> Result<SkinView, CommandError> {
    let root = state.library.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        crate::skins::lookup(&root, &nickname, refresh).map_err(CommandError::from)
    })
    .await
    .map_err(|_| CommandError::from(CoreError::Worker))?
}
#[tauri::command]
pub async fn configure_instance_launch(
    request: ConfigureLaunch,
    state: tauri::State<'_, AppState>,
) -> Result<LibraryChange, CommandError> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        library
            .configure_launch(request)
            .map_err(CommandError::from)
    })
    .await
    .map_err(|_| CommandError::from(CoreError::Worker))?
}
