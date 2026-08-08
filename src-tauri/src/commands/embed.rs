use tauri::{AppHandle, State};

use crate::error::{AppError, AppResult};
use crate::models::EmbedStatus;
use crate::services::embed::{self, EmbedState};
use crate::services::store::{self, DbState};

#[tauri::command]
pub fn get_embed_status(
    db: State<'_, DbState>,
    state: State<'_, EmbedState>,
) -> AppResult<EmbedStatus> {
    store::with_conn(&db, |conn| embed::get_embed_status(conn, &state))
}

#[tauri::command]
pub fn start_embedding(app: AppHandle, state: State<'_, EmbedState>) -> AppResult<()> {
    if state.running.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(AppError::ollama("embedding pipeline is already running"));
    }
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = embed::run_embed_pipeline(app_handle).await;
    });
    Ok(())
}
