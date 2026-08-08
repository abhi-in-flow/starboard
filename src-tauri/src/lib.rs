mod commands;
mod error;
mod models;
mod services;

use tauri::Manager;

use commands::{auth, categorize, embed, repos, settings, sync};
use services::categorizer::CategorizeState;
use services::embed::EmbedState;
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
            app.manage(CategorizeState::default());
            app.manage(EmbedState::default());
            // Resume unfinished README work from a previous session.
            services::sync::spawn_readme_queue_if_needed(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::update_settings,
            settings::rebuild_embeddings_table,
            auth::get_auth_status,
            auth::connect_github,
            auth::disconnect_github,
            sync::start_sync,
            sync::resume_readme_queue,
            sync::get_sync_status,
            repos::list_repos,
            repos::search_repos,
            repos::get_repo,
            repos::get_library_facets,
            repos::list_categories,
            categorize::get_ollama_status,
            categorize::generate_taxonomy,
            categorize::get_taxonomy_edit,
            categorize::update_taxonomy,
            categorize::commit_taxonomy,
            categorize::start_assignment,
            categorize::get_categorize_status,
            categorize::set_repo_category,
            categorize::recategorize_repo,
            embed::get_embed_status,
            embed::start_embedding,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
