use crate::{
    commands::AppState,
    error::{CommandError, CoreError},
    instances::{Library, model::*},
};

async fn work<T: Send + 'static>(
    library: Library,
    operation: impl FnOnce(Library) -> Result<T, CoreError> + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(move || operation(library).map_err(CommandError::from))
        .await
        .map_err(|_| CommandError::from(CoreError::Worker))?
}

#[tauri::command]
pub async fn instance_summary(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<crate::instances::summary::InstanceSummary, CommandError> {
    work(state.library.clone(), move |library| library.summary(&id)).await
}

#[tauri::command]
pub async fn instance_icon(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, CommandError> {
    work(state.library.clone(), move |library| {
        library.icon_image(&id)
    })
    .await
}
#[tauri::command]
pub fn instance_icon_catalog() -> Vec<crate::instances::icons::InstanceLogo> {
    crate::instances::icons::catalog()
}
#[tauri::command]
pub async fn set_instance_icon(
    request: RecordRequest,
    choice: String,
    state: tauri::State<'_, AppState>,
) -> Result<LibraryChange, CommandError> {
    work(state.library.clone(), move |library| {
        library.select_icon(request, &choice)
    })
    .await
}
#[tauri::command]
pub async fn pick_instance_icon(
    request: RecordRequest,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<Option<LibraryChange>, CommandError> {
    work(state.library.clone(), move |library| {
        let Some(path) = rfd::FileDialog::new()
            .set_parent(&window)
            .add_filter("Image", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
        else {
            return Ok(None);
        };
        library.custom_icon(request, &path).map(Some)
    })
    .await
}

#[tauri::command]
pub async fn library_snapshot(
    state: tauri::State<'_, AppState>,
) -> Result<LibrarySnapshot, CommandError> {
    work(state.library.clone(), |library| library.snapshot()).await
}

#[tauri::command]
pub async fn create_instance(
    request: CreateInstance,
    state: tauri::State<'_, AppState>,
) -> Result<LibraryChange, CommandError> {
    work(state.library.clone(), |library| library.create(request)).await
}

#[tauri::command]
pub async fn update_instance(
    request: UpdateInstance,
    state: tauri::State<'_, AppState>,
) -> Result<LibraryChange, CommandError> {
    work(state.library.clone(), |library| library.update(request)).await
}

#[tauri::command]
pub async fn duplicate_instance(
    request: DuplicateInstance,
    state: tauri::State<'_, AppState>,
) -> Result<LibraryChange, CommandError> {
    work(state.library.clone(), |library| library.duplicate(request)).await
}

#[tauri::command]
pub async fn delete_instance(
    request: DeleteInstance,
    state: tauri::State<'_, AppState>,
) -> Result<LibraryChange, CommandError> {
    work(state.library.clone(), |library| library.delete(request)).await
}

#[tauri::command]
pub async fn save_collection(
    request: SaveCollection,
    state: tauri::State<'_, AppState>,
) -> Result<LibraryChange, CommandError> {
    work(state.library.clone(), |library| {
        library.save_collection(request)
    })
    .await
}

#[tauri::command]
pub async fn delete_collection(
    request: RecordRequest,
    state: tauri::State<'_, AppState>,
) -> Result<LibraryChange, CommandError> {
    work(state.library.clone(), |library| {
        library.delete_collection(request)
    })
    .await
}

#[tauri::command]
pub async fn open_library_folder(
    request: OpenFolder,
    state: tauri::State<'_, AppState>,
) -> Result<(), CommandError> {
    work(state.library.clone(), |library| {
        library.open_folder(request)
    })
    .await
}

#[tauri::command]
pub async fn set_instance_favorite(
    request: SetFavorite,
    state: tauri::State<'_, AppState>,
) -> Result<LibraryChange, CommandError> {
    work(state.library.clone(), |library| {
        library.set_favorite(request)
    })
    .await
}

#[tauri::command]
pub async fn instance_shortcut_plan(
    request: RecordRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ShortcutPlan, CommandError> {
    work(state.library.clone(), |library| {
        library.shortcut_plan(request)
    })
    .await
}
