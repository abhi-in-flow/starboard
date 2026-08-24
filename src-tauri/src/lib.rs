mod commands;
mod error;
mod models;
mod services;

// Keep sanitizer reachable from the library crate (used by README UI + tests).
#[allow(unused_imports)]
pub use services::sanitize::is_safe_markdown_url;

use tauri::Manager;

use commands::{auth, categorize, embed, insights, repos, settings, status, sync};
use services::categorizer::CategorizeState;
use services::embed::EmbedState;
use services::store::{self, DbState};
use services::sync::SyncState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    services::logging::init_logger();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let path = store::db_path(app.handle())?;
            let conn = store::open_and_migrate(&path)?;
            app.manage(DbState(std::sync::Mutex::new(conn)));
            app.manage(SyncState::default());
            app.manage(CategorizeState::default());
            app.manage(EmbedState::default());
            // Resume unfinished README work from a previous session.
            // When the README queue is empty or finishes, it kicks silent
            // auto-categorize (eligible uncategorized repos) and auto-embed
            // (see spawn_readme_queue_if_needed).
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
            sync::cancel_sync,
            sync::cancel_readme_queue,
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
            categorize::cancel_assignment,
            categorize::get_categorize_status,
            categorize::set_repo_category,
            categorize::recategorize_repo,
            embed::get_embed_status,
            embed::start_embedding,
            embed::cancel_embedding,
            insights::get_insights,
            insights::get_interest_drift_drilldown,
            insights::get_library_export,
            insights::write_library_export,
            status::get_system_status,
            status::check_db_integrity,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
