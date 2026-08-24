use tauri::{AppHandle, State};

use crate::error::{AppError, AppResult};
use crate::models::{
    AssignRepoCategoryRequest, CategorizeStatus, OllamaStatus, TaxonomyDraft, TaxonomyEdit,
};
use crate::services::categorizer::{self, CategorizeState};
use crate::services::ollama::OllamaClient;
use crate::services::settings;
use crate::services::store::{self, DbState};

#[tauri::command]
pub async fn get_ollama_status(db: State<'_, DbState>) -> AppResult<OllamaStatus> {
    let app_settings = store::with_conn(&db, settings::get_settings)?;
    match OllamaClient::from_settings(&app_settings) {
        Ok(client) => client.health_check().await,
        Err(e) => Ok(OllamaStatus {
            available: false,
            message: e.message,
        }),
    }
}

#[tauri::command]
pub async fn generate_taxonomy(app: AppHandle) -> AppResult<TaxonomyDraft> {
    categorizer::generate_taxonomy(&app).await
}

#[tauri::command]
pub fn get_taxonomy_edit(db: State<'_, DbState>) -> AppResult<TaxonomyEdit> {
    store::with_conn(&db, categorizer::load_taxonomy_edit)
}

#[tauri::command]
pub fn update_taxonomy(db: State<'_, DbState>, edit: TaxonomyEdit) -> AppResult<()> {
    store::with_conn(&db, |conn| categorizer::update_taxonomy(conn, &edit))
}

#[tauri::command]
pub fn commit_taxonomy(
    db: State<'_, DbState>,
    draft: TaxonomyDraft,
    force: Option<bool>,
) -> AppResult<()> {
    store::with_conn(&db, |conn| {
        categorizer::commit_taxonomy(conn, &draft, force.unwrap_or(false))
    })
}

#[tauri::command]
pub fn start_assignment(app: AppHandle, state: State<'_, CategorizeState>) -> AppResult<()> {
    if state.running.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(AppError::ollama("categorization is already running"));
    }
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = categorizer::run_assignment(app_handle).await;
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_assignment(state: State<'_, CategorizeState>) -> AppResult<()> {
    categorizer::request_cancel(&state)
}

#[tauri::command]
pub fn get_categorize_status(state: State<'_, CategorizeState>) -> AppResult<CategorizeStatus> {
    let running = state.running.load(std::sync::atomic::Ordering::SeqCst);
    let last_error = state
        .last_error
        .lock()
        .map_err(|_| AppError::db("categorize state lock poisoned"))?
        .clone();
    Ok(CategorizeStatus {
        running,
        last_error,
    })
}

#[tauri::command]
pub fn set_repo_category(
    db: State<'_, DbState>,
    request: AssignRepoCategoryRequest,
) -> AppResult<()> {
    store::with_conn(&db, |conn| {
        categorizer::set_manual_category(conn, request.repo_id, request.category_id)
    })
}

#[tauri::command]
pub async fn recategorize_repo(app: AppHandle, repo_id: i64) -> AppResult<()> {
    categorizer::recategorize_single_repo(&app, repo_id).await
}
