use tauri::State;

use crate::error::AppResult;
use crate::models::{IntegrityReport, SystemStatus};
use crate::services::store::{self, DbState};
use crate::services::{integrity, status};

#[tauri::command]
pub fn get_system_status(state: State<'_, DbState>) -> AppResult<SystemStatus> {
    store::with_conn(&state, status::get_system_status)
}

#[tauri::command]
pub fn check_db_integrity(state: State<'_, DbState>) -> AppResult<IntegrityReport> {
    store::with_conn(&state, integrity::check_integrity)
}
