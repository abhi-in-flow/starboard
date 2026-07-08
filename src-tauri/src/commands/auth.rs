use tauri::State;

use crate::error::AppResult;
use crate::models::AuthStatus;
use crate::services::{github_auth, secrets, settings, store};
use crate::services::store::DbState;

#[tauri::command]
pub fn get_auth_status(state: State<'_, DbState>) -> AppResult<AuthStatus> {
    let pat = secrets::get_pat()?;
    let username = store::with_conn(&state, settings::get_settings)?.github_username;

    Ok(AuthStatus {
        connected: pat.is_some() && username.is_some(),
        username,
    })
}

#[tauri::command]
pub async fn connect_github(
    state: State<'_, DbState>,
    pat: String,
) -> AppResult<AuthStatus> {
    let user = github_auth::validate_pat(&pat).await?;
    secrets::store_pat(&pat)?;
    store::with_conn(&state, |conn| settings::set_github_username(conn, &user.login))?;

    Ok(AuthStatus {
        connected: true,
        username: Some(user.login),
    })
}

#[tauri::command]
pub fn disconnect_github(state: State<'_, DbState>) -> AppResult<AuthStatus> {
    secrets::delete_pat()?;
    store::with_conn(&state, settings::clear_github_username)?;

    Ok(AuthStatus {
        connected: false,
        username: None,
    })
}
