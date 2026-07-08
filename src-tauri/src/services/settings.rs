use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::models::{AppSettings, UpdateSettingsRequest};

const KEY_OLLAMA_BASE_URL: &str = "ollama_base_url";
const KEY_OLLAMA_CHAT_MODEL: &str = "ollama_chat_model";
const KEY_OLLAMA_EMBED_MODEL: &str = "ollama_embed_model";
const KEY_GITHUB_USERNAME: &str = "github_username";
pub const KEY_STARRED_ETAG: &str = "starred_etag";
pub const KEY_LAST_SYNCED_AT: &str = "last_synced_at";

pub fn get_settings(conn: &Connection) -> AppResult<AppSettings> {
    let defaults = AppSettings::default();
    Ok(AppSettings {
        ollama_base_url: get_value(conn, KEY_OLLAMA_BASE_URL)?
            .unwrap_or(defaults.ollama_base_url),
        ollama_chat_model: get_value(conn, KEY_OLLAMA_CHAT_MODEL)?
            .unwrap_or(defaults.ollama_chat_model),
        ollama_embed_model: get_value(conn, KEY_OLLAMA_EMBED_MODEL)?
            .unwrap_or(defaults.ollama_embed_model),
        github_username: get_value(conn, KEY_GITHUB_USERNAME)?,
    })
}

pub fn update_settings(conn: &Connection, req: UpdateSettingsRequest) -> AppResult<AppSettings> {
    if let Some(url) = req.ollama_base_url {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err(AppError::settings("ollama_base_url cannot be empty"));
        }
        set_value(conn, KEY_OLLAMA_BASE_URL, trimmed)?;
    }
    if let Some(model) = req.ollama_chat_model {
        set_value(conn, KEY_OLLAMA_CHAT_MODEL, model.trim())?;
    }
    if let Some(model) = req.ollama_embed_model {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return Err(AppError::settings("ollama_embed_model cannot be empty"));
        }
        set_value(conn, KEY_OLLAMA_EMBED_MODEL, trimmed)?;
    }
    get_settings(conn)
}

pub fn set_github_username(conn: &Connection, username: &str) -> AppResult<()> {
    set_value(conn, KEY_GITHUB_USERNAME, username)
}

pub fn clear_github_username(conn: &Connection) -> AppResult<()> {
    conn.execute(
        "DELETE FROM settings WHERE key = ?1",
        [KEY_GITHUB_USERNAME],
    )?;
    Ok(())
}

pub fn get_value(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query([key])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

pub fn set_value(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::store::open_and_migrate;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_conn() -> Connection {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("starboard_settings_{nanos}.db"));
        open_and_migrate(&path).expect("migrate")
    }

    #[test]
    fn defaults_and_update_roundtrip() {
        let conn = test_conn();
        let settings = get_settings(&conn).expect("get");
        assert_eq!(settings.ollama_base_url, "http://127.0.0.1:11434");
        assert_eq!(settings.ollama_embed_model, "nomic-embed-text");
        assert!(settings.github_username.is_none());

        let updated = update_settings(
            &conn,
            UpdateSettingsRequest {
                ollama_base_url: Some("http://192.168.1.10:11434".into()),
                ollama_chat_model: Some("qwen3:14b".into()),
                ollama_embed_model: None,
            },
        )
        .expect("update");

        assert_eq!(updated.ollama_base_url, "http://192.168.1.10:11434");
        assert_eq!(updated.ollama_chat_model, "qwen3:14b");
        assert_eq!(updated.ollama_embed_model, "nomic-embed-text");
    }
}
