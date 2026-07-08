use tauri::{AppHandle, State};

use crate::error::{AppError, AppResult};
use crate::models::{SyncResult, SyncStatus};
use crate::services::store::{self, DbState};
use crate::services::sync::{self, SyncState};

#[tauri::command]
pub async fn start_sync(app: AppHandle, full: Option<bool>) -> AppResult<SyncResult> {
    sync::run_sync(app, full.unwrap_or(false)).await
}

#[tauri::command]
pub fn resume_readme_queue(
    app: AppHandle,
    db: State<'_, DbState>,
    sync_state: State<'_, SyncState>,
) -> AppResult<()> {
    if sync_state
        .readme_running
        .load(std::sync::atomic::Ordering::SeqCst)
    {
        return Err(AppError::sync("README queue is already running"));
    }
    let pending = store::with_conn(&db, |conn| {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM repos WHERE readme_excerpt IS NULL AND unstarred = 0",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    })?;
    if pending == 0 {
        return Err(AppError::sync("no pending README excerpts to fetch"));
    }
    sync::spawn_readme_queue_if_needed(app);
    Ok(())
}

#[tauri::command]
pub fn get_sync_status(
    db: State<'_, DbState>,
    sync_state: State<'_, SyncState>,
) -> AppResult<SyncStatus> {
    let running = sync_state.running.load(std::sync::atomic::Ordering::SeqCst);
    let readme_running = sync_state
        .readme_running
        .load(std::sync::atomic::Ordering::SeqCst);
    store::with_conn(&db, |conn| sync::get_sync_status(conn, running, readme_running))
}
