use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::models::{AuthStatus, GitHubUser};
use crate::services::{secrets, settings};

const USER_AGENT: &str = "Starboard/0.1 (+https://github.com/brocode/starboard)";

pub async fn validate_pat(pat: &str) -> AppResult<GitHubUser> {
    let trimmed = pat.trim();
    if trimmed.is_empty() {
        return Err(AppError::auth("PAT cannot be empty"));
    }

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(crate::services::github::GitHubClient::CONNECT_TIMEOUT)
        .timeout(crate::services::github::GitHubClient::REQUEST_TIMEOUT)
        .build()?;

    let response = client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {trimmed}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await?;

    let status = response.status();
    if status.is_success() {
        let user = response.json::<GitHubUser>().await?;
        return Ok(user);
    }

    let body = response.text().await.unwrap_or_default();
    let message = extract_github_message(&body).unwrap_or_else(|| {
        if status.as_u16() == 401 {
            "Bad credentials".to_string()
        } else {
            format!("GitHub returned HTTP {status}")
        }
    });

    Err(AppError::auth(message))
}

/// Persist a validated connection. PAT is written to the keyring first; if the
/// username write fails, the PAT is deleted so keyring and settings cannot diverge.
pub fn persist_github_connection(conn: &Connection, pat: &str, username: &str) -> AppResult<()> {
    secrets::store_pat(pat)?;
    if let Err(e) = settings::set_github_username(conn, username) {
        let _ = secrets::delete_pat();
        return Err(e);
    }
    Ok(())
}

/// Disconnect both sides. Prefer clearing the PAT even if the username delete fails.
pub fn disconnect_github(conn: &Connection) -> AppResult<()> {
    let keyring_err = secrets::delete_pat().err();
    let db_err = settings::clear_github_username(conn).err();
    if let Some(e) = keyring_err.or(db_err) {
        return Err(e);
    }
    Ok(())
}

/// Reconcile a stale username left behind if the keyring entry disappeared.
pub fn auth_status(conn: &Connection) -> AppResult<AuthStatus> {
    let pat = secrets::get_pat()?;
    let username = settings::get_settings(conn)?.github_username;
    match (pat.is_some(), username) {
        (true, Some(u)) => Ok(AuthStatus {
            connected: true,
            username: Some(u),
        }),
        (false, Some(_)) => {
            settings::clear_github_username(conn)?;
            Ok(AuthStatus {
                connected: false,
                username: None,
            })
        }
        (true, None) => Ok(AuthStatus {
            connected: false,
            username: None,
        }),
        (false, None) => Ok(AuthStatus {
            connected: false,
            username: None,
        }),
    }
}

fn extract_github_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value
        .get("message")
        .and_then(|m| m.as_str())
        .map(str::to_string)
}
