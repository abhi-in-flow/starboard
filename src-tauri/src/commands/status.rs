use tauri::State;

use crate::error::AppResult;
use crate::models::SystemStatus;
use crate::services::status;
use crate::services::store::{self, DbState};

#[tauri::command]
pub fn get_system_status(state: State<'_, DbState>) -> AppResult<SystemStatus> {
    store::with_conn(&state, status::get_system_status)
}
