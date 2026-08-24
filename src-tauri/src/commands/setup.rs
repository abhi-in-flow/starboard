use tauri::State;

use crate::error::AppResult;
use crate::models::SetupStatus;
use crate::services::store::DbState;
use crate::services::{secrets, settings, setup, store};

#[tauri::command]
pub fn get_setup_status(state: State<'_, DbState>) -> AppResult<SetupStatus> {
    let pat_present = secrets::get_pat()?.is_some();
    store::with_conn(&state, |conn| setup::get_setup_status(conn, pat_present))
}

#[tauri::command]
pub fn set_onboarding_completed(
    state: State<'_, DbState>,
    completed: bool,
) -> AppResult<SetupStatus> {
    let pat_present = secrets::get_pat()?.is_some();
    store::with_conn(&state, |conn| {
        settings::set_onboarding_completed(conn, completed)?;
        setup::get_setup_status(conn, pat_present)
    })
}
