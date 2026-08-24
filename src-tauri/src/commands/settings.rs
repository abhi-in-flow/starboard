use tauri::State;

use crate::error::AppResult;
use crate::models::{AppSettings, UpdateSettingsRequest};
use crate::services::store::DbState;
use crate::services::{settings, store};

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

#[tauri::command]
pub fn rebuild_embeddings_table(
    state: State<'_, DbState>,
    dimension: Option<i64>,
) -> AppResult<AppSettings> {
    store::with_conn(&state, |conn| {
        let dim = match dimension {
            Some(d) => d,
            None => settings::get_embed_dimension(conn)?,
        };
        settings::apply_embed_dimension_rebuild(conn, dim)
    })
}
