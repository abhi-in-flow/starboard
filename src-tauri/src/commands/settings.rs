use tauri::State;

use crate::error::AppResult;
use crate::models::{AppSettings, UpdateSettingsRequest};
use crate::services::{settings, store};
use crate::services::store::DbState;

#[tauri::command]
pub fn get_settings(state: State<'_, DbState>) -> AppResult<AppSettings> {
    store::with_conn(&state, settings::get_settings)
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, DbState>,
    request: UpdateSettingsRequest,
) -> AppResult<AppSettings> {
    store::with_conn(&state, |conn| settings::update_settings(conn, request))
}
