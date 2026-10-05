use crate::{
    commands::AppState,
    error::{CommandError, CoreError},
    game::{GameManager, model::*},
};

async fn work<T: Send + 'static>(
    game: GameManager,
    action: impl FnOnce(GameManager) -> Result<T, CoreError> + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(move || action(game).map_err(CommandError::from))
        .await
        .map_err(|_| CommandError::from(CoreError::Worker))?
}
#[tauri::command]
pub async fn pause_downloads(
    paused: bool,
    state: tauri::State<'_, AppState>,
) -> Result<GameState, CommandError> {
    work(state.game.clone(), move |game| game.pause_downloads(paused)).await
}
#[tauri::command]
pub async fn download_cache(
    cleanup: bool,
    state: tauri::State<'_, AppState>,
) -> Result<crate::game::cache::CacheStats, CommandError> {
    work(state.game.clone(), move |game| {
        if cleanup {
            if game.is_active() {
                return Err(CoreError::InstanceBusy);
            }
            crate::game::cache::trim(game.library.root(), 0)
        } else {
            crate::game::cache::stats(game.library.root())
        }
    })
    .await
}
#[tauri::command]
pub async fn loader_versions(
    loader: crate::instances::model::Loader,
    minecraft: String,
    state: tauri::State<'_, AppState>,
) -> Result<crate::game::loaders::LoaderCatalog, CommandError> {
    work(state.game.clone(), move |game| {
        crate::game::loaders::catalog(game.library.root(), loader, &minecraft)
    })
    .await
}
#[tauri::command]
pub async fn game_catalog(
    refresh: bool,
    state: tauri::State<'_, AppState>,
) -> Result<VersionCatalog, CommandError> {
    work(state.game.clone(), move |game| game.catalog(refresh)).await
}
#[tauri::command]
pub async fn game_state(state: tauri::State<'_, AppState>) -> Result<GameState, CommandError> {
    work(state.game.clone(), |game| Ok(game.snapshot())).await
}
#[tauri::command]
pub async fn start_game(
    request: GameRequest,
    state: tauri::State<'_, AppState>,
) -> Result<GameState, CommandError> {
    work(state.game.clone(), |game| game.start(request)).await
}
#[tauri::command]
pub async fn cancel_game_operation(state: tauri::State<'_, AppState>) -> Result<(), CommandError> {
    state.game.cancel();
    Ok(())
}
#[tauri::command]
pub async fn stop_game(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<GameState, CommandError> {
    work(state.game.clone(), move |game| game.stop(&id)).await
}
#[tauri::command]
pub async fn java_runtimes(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<JavaRuntime>, CommandError> {
    work(state.game.clone(), |game| game.java_runtimes()).await
}
#[tauri::command]
pub async fn inspect_java(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<JavaRuntime, CommandError> {
    work(state.game.clone(), move |game| game.inspect_java(&path)).await
}
