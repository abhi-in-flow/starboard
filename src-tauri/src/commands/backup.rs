use std::sync::atomic::Ordering;

use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::models::{BackupResult, BackupValidation, RestoreResult};
use crate::services::backup::{self, JobGuard};
use crate::services::categorizer::CategorizeState;
use crate::services::embed::EmbedState;
use crate::services::store::{self, DbState};
use crate::services::sync::SyncState;

#[tauri::command]
pub fn create_backup(
    app: AppHandle,
    state: State<'_, DbState>,
    path: String,
) -> AppResult<BackupResult> {
    let live = store::db_path(&app)?;
    let dest = backup::validate_backup_dest_path(path.trim(), Some(live.as_path()))?;
    store::with_conn(&state, |conn| {
        backup::create_backup(conn, &dest, Some(live.as_path()))
    })
}

#[tauri::command]
pub fn validate_backup(path: String) -> AppResult<BackupValidation> {
    let src = backup::validate_backup_source_path(path.trim(), None)?;
    backup::validate_backup_file(&src)
}

#[tauri::command]
pub fn restore_backup(
    app: AppHandle,
    db: State<'_, DbState>,
    sync_state: State<'_, SyncState>,
    embed_state: State<'_, EmbedState>,
    categorize_state: State<'_, CategorizeState>,
    path: String,
) -> AppResult<RestoreResult> {
    let jobs = JobGuard {
        sync: sync_state.running.load(Ordering::SeqCst),
        readme: sync_state.readme_running.load(Ordering::SeqCst),
        embed: embed_state.running.load(Ordering::SeqCst),
        categorize: categorize_state.running.load(Ordering::SeqCst),
    };
    let live = store::db_path(&app)?;
    let src = backup::validate_backup_source_path(path.trim(), Some(live.as_path()))?;
    store::with_conn_mut(&db, |conn| backup::restore_backup(conn, &src, &live, &jobs))
}
