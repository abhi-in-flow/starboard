mod commands;
mod error;
mod models;
mod services;

use tauri::Manager;

use commands::{auth, settings, sync};
use services::store::{self, DbState};
use services::sync::SyncState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let path = store::db_path(app.handle())?;
            let conn = store::open_and_migrate(&path)?;
            app.manage(DbState(std::sync::Mutex::new(conn)));
            app.manage(SyncState::default());
            // Resume unfinished README work from a previous session.
            services::sync::spawn_readme_queue_if_needed(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::update_settings,
            auth::get_auth_status,
            auth::connect_github,
            auth::disconnect_github,
            sync::start_sync,
            sync::resume_readme_queue,
            sync::get_sync_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
