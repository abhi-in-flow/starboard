use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::models::{IntegrityReport, SystemStatus};
use crate::services::store::{self, DbState};
use crate::services::{integrity, status};

#[tauri::command]
pub fn get_system_status(app: AppHandle, state: State<'_, DbState>) -> AppResult<SystemStatus> {
    let db_path = store::db_path(&app)?;
    store::with_conn(&state, |conn| {
        status::get_system_status(conn, &db_path.to_string_lossy())
    })
}

#[tauri::command]
pub fn check_db_integrity(state: State<'_, DbState>) -> AppResult<IntegrityReport> {
    store::with_conn(&state, integrity::check_integrity)
}
