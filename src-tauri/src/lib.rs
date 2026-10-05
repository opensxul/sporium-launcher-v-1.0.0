pub mod app_updates;
pub mod commands;
pub mod content;
mod content_commands;
pub mod error;
pub mod game;
mod game_commands;
mod instance_commands;
pub mod instances;
mod logging;
mod pack_commands;
pub mod packs;
mod profile_commands;
pub mod profiles;
mod project_commands;
pub mod projects;
pub mod settings;
pub mod skins;
pub mod storage;

use commands::{AppInfo, AppState};
use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_updates::Updates::default())
        .setup(|app| {
            let data_directory = app.path().app_local_data_dir()?;
            // Debug-only isolation for native smoke tests; production always uses the OS directory.
            #[cfg(debug_assertions)]
            let data_directory = std::env::var_os("SPORIUM_TEST_DATA_DIR")
                .map(std::path::PathBuf::from)
                .filter(|path| path.is_absolute())
                .unwrap_or(data_directory);

            let log_guard = logging::init(&data_directory.join("launcher/logs")).ok();
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "app_started");
            let database = storage::Database::new(data_directory.join("launcher/sporium.sqlite3"));
            let library = instances::Library::new(data_directory.clone(), database.clone());
            let packs = packs::PackManager::new(library.clone());
            let projects = projects::ProjectManager::new(library.clone(), packs.clone())?;
            let content = content::ContentManager::new(library.clone())?;
            content.start_automatic_worker(projects.clone())?;
            app.manage(AppState {
                projects,
                packs,
                content,
                game: game::GameManager::new(library.clone(), database.clone()),
                library: instances::Library::new(data_directory.clone(), database.clone()),
                database,
                info: AppInfo::new(data_directory.to_string_lossy().into_owned()),
                _log_guard: log_guard,
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && (window
                    .state::<app_updates::Updates>()
                    .busy
                    .load(std::sync::atomic::Ordering::SeqCst)
                    || window.state::<AppState>().packs.is_active()
                    || window.state::<AppState>().projects.is_active()
                    || window.state::<AppState>().game.is_active()
                    || window.state::<AppState>().content.is_active())
            {
                api.prevent_close();
                let _ = window.emit("game-close-blocked", ());
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_updates::app_update_check,
            app_updates::app_update_install,
            project_commands::project_view,
            project_commands::project_replan,
            project_commands::studio_files,
            project_commands::studio_plan,
            project_commands::project_check,
            project_commands::project_repair_plan,
            project_commands::project_pack_plan,
            project_commands::project_source_plan,
            project_commands::project_pick_manifest,
            project_commands::project_export,
            project_commands::project_apply,
            project_commands::project_dismiss,
            project_commands::project_cancel,
            content_commands::automatic_policy,
            content_commands::save_automatic_policy,
            content_commands::automatic_reports,
            content_commands::automatic_check,
            pack_commands::provider_pack_preview,
            pack_commands::pack_preview,
            pack_commands::pick_pack,
            pack_commands::pack_import,
            pack_commands::pack_dismiss,
            pack_commands::pack_state,
            pack_commands::pack_cancel,
            pack_commands::pack_opening,
            pack_commands::external_scan,
            pack_commands::pick_external,
            pack_commands::external_preview,
            pack_commands::pack_export,
            pack_commands::pick_pack_export,
            pack_commands::register_pack_formats,
            content_commands::content_search,
            content_commands::content_worlds,
            content_commands::world_archive_plan,
            content_commands::pick_world_archive,
            content_commands::finish_world_archive,
            content_commands::world_project_plan,
            content_commands::local_content_dependencies,
            content_commands::content_dependency_plan,
            content_commands::content_tags,
            content_commands::content_icon,
            content_commands::content_details,
            content_commands::content_plan,
            content_commands::content_create_plan,
            content_commands::content_install,
            content_commands::content_state,
            content_commands::content_cancel,
            content_commands::installed_content,
            content_commands::untracked_content,
            content_commands::change_content,
            content_commands::content_history,
            content_commands::content_updates,
            content_commands::content_restore_points,
            content_commands::restore_content,
            content_commands::content_update_plan,
            content_commands::content_update_policy,
            content_commands::local_content_plan,
            content_commands::content_adoption_plan,
            content_commands::content_diagnostics,
            content_commands::finish_content_adoption,
            content_commands::local_content_icon,
            content_commands::pick_local_content,
            content_commands::finish_local_content,
            content_commands::open_content_project,
            commands::bootstrap,
            commands::save_settings,
            profile_commands::edit_profile,
            profile_commands::profile_skin,
            profile_commands::configure_instance_launch,
            game_commands::loader_versions,
            game_commands::pause_downloads,
            game_commands::download_cache,
            instance_commands::library_snapshot,
            instance_commands::create_instance,
            instance_commands::update_instance,
            instance_commands::duplicate_instance,
            instance_commands::delete_instance,
            instance_commands::save_collection,
            instance_commands::delete_collection,
            instance_commands::open_library_folder,
            instance_commands::set_instance_favorite,
            instance_commands::instance_shortcut_plan,
            instance_commands::instance_icon,
            instance_commands::instance_summary,
            instance_commands::instance_icon_catalog,
            instance_commands::set_instance_icon,
            instance_commands::pick_instance_icon,
            game_commands::game_catalog,
            game_commands::game_state,
            game_commands::start_game,
            game_commands::cancel_game_operation,
            game_commands::stop_game,
            game_commands::java_runtimes,
            game_commands::inspect_java
        ])
        .run(tauri::generate_context!())
        .expect("Sporium failed to initialize its desktop window");
}
