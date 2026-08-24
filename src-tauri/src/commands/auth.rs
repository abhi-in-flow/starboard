use tauri::State;

use crate::error::AppResult;
use crate::models::AuthStatus;
use crate::services::github_auth;
use crate::services::store::{self, DbState};

#[tauri::command]
pub fn get_auth_status(state: State<'_, DbState>) -> AppResult<AuthStatus> {
    store::with_conn(&state, github_auth::auth_status)
}

#[tauri::command]
pub async fn connect_github(state: State<'_, DbState>, pat: String) -> AppResult<AuthStatus> {
    let user = github_auth::validate_pat(&pat).await?;
    store::with_conn(&state, |conn| {
        github_auth::persist_github_connection(conn, &pat, &user.login)
    })?;
    Ok(AuthStatus {
        connected: true,
        username: Some(user.login),
    })
}

#[tauri::command]
pub fn disconnect_github(state: State<'_, DbState>) -> AppResult<AuthStatus> {
    store::with_conn(&state, github_auth::disconnect_github)?;
    Ok(AuthStatus {
        connected: false,
        username: None,
    })
}
