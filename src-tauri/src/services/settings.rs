use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::models::{AppSettings, UpdateSettingsRequest};
use crate::services::store::{self, DEFAULT_EMBED_DIMENSION};

const KEY_OLLAMA_BASE_URL: &str = "ollama_base_url";
const KEY_OLLAMA_CHAT_MODEL: &str = "ollama_chat_model";
const KEY_OLLAMA_EMBED_MODEL: &str = "ollama_embed_model";
const KEY_GITHUB_USERNAME: &str = "github_username";
pub const KEY_STARRED_ETAG: &str = "starred_etag";
pub const KEY_LAST_SYNCED_AT: &str = "last_synced_at";
pub const KEY_LAST_FULL_RECONCILE_AT: &str = "last_full_reconcile_at";
pub const KEY_INCREMENTAL_SINCE_RECONCILE: &str = "incremental_syncs_since_reconcile";
pub const KEY_EMBED_DIMENSION: &str = "embed_dimension";
/// Dimension the live `repo_embeddings` vec0 table was created with.
pub const KEY_EMBEDDINGS_TABLE_DIMENSION: &str = "embeddings_table_dimension";

pub fn get_settings(conn: &Connection) -> AppResult<AppSettings> {
    let defaults = AppSettings::default();
    Ok(AppSettings {
        ollama_base_url: get_value(conn, KEY_OLLAMA_BASE_URL)?.unwrap_or(defaults.ollama_base_url),
        ollama_chat_model: get_value(conn, KEY_OLLAMA_CHAT_MODEL)?
            .unwrap_or(defaults.ollama_chat_model),
        ollama_embed_model: get_value(conn, KEY_OLLAMA_EMBED_MODEL)?
            .unwrap_or(defaults.ollama_embed_model),
        embed_dimension: get_embed_dimension(conn)?,
        github_username: get_value(conn, KEY_GITHUB_USERNAME)?,
        embeddings_need_rebuild: embeddings_need_rebuild(conn)?,
    })
}

pub fn get_embed_dimension(conn: &Connection) -> AppResult<i64> {
    match get_value(conn, KEY_EMBED_DIMENSION)? {
        Some(raw) => raw
            .parse::<i64>()
            .map_err(|_| AppError::settings(format!("invalid embed_dimension: {raw}"))),
        None => Ok(DEFAULT_EMBED_DIMENSION),
    }
}

pub fn get_embeddings_table_dimension(conn: &Connection) -> AppResult<Option<i64>> {
    match get_value(conn, KEY_EMBEDDINGS_TABLE_DIMENSION)? {
        Some(raw) => {
            let parsed = raw.parse::<i64>().map_err(|_| {
                AppError::settings(format!("invalid embeddings_table_dimension: {raw}"))
            })?;
            Ok(Some(parsed))
        }
        None => {
            // Migration 002 creates float[768]; treat missing key as that default when table exists.
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'repo_embeddings'",
                [],
                |row| row.get(0),
            )?;
            if exists {
                Ok(Some(DEFAULT_EMBED_DIMENSION))
            } else {
                Ok(None)
            }
        }
    }
}

pub fn embeddings_need_rebuild(conn: &Connection) -> AppResult<bool> {
    let wanted = get_embed_dimension(conn)?;
    match get_embeddings_table_dimension(conn)? {
        Some(have) => Ok(have != wanted),
        None => Ok(true),
    }
}

pub fn update_settings(conn: &Connection, req: UpdateSettingsRequest) -> AppResult<AppSettings> {
    crate::services::store::with_tx(conn, |tx| {
        if let Some(url) = req.ollama_base_url {
            let trimmed = validate_ollama_base_url(&url)?;
            set_value(tx, KEY_OLLAMA_BASE_URL, &trimmed)?;
        }
        if let Some(model) = req.ollama_chat_model {
            set_value(tx, KEY_OLLAMA_CHAT_MODEL, model.trim())?;
        }
        if let Some(model) = req.ollama_embed_model {
            let trimmed = model.trim();
            if trimmed.is_empty() {
                return Err(AppError::settings("ollama_embed_model cannot be empty"));
            }
            set_value(tx, KEY_OLLAMA_EMBED_MODEL, trimmed)?;
        }
        if let Some(dimension) = req.embed_dimension {
            if !(1..=8192).contains(&dimension) {
                return Err(AppError::settings(format!(
                    "embed_dimension out of range: {dimension}"
                )));
            }
            set_value(tx, KEY_EMBED_DIMENSION, &dimension.to_string())?;
        }
        Ok(())
    })?;
    get_settings(conn)
}

/// http/https only; reject embedded credentials; LAN hosts are allowed.
pub fn validate_ollama_base_url(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::settings("ollama_base_url cannot be empty"));
    }
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|e| AppError::settings(format!("invalid ollama_base_url: {e}")))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(AppError::settings("ollama_base_url must use http or https"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::settings(
            "ollama_base_url must not include credentials",
        ));
    }
    if parsed.host_str().is_none() {
        return Err(AppError::settings("ollama_base_url is missing a host"));
    }
    Ok(trimmed.trim_end_matches('/').to_string())
}

/// Persist a new embed dimension and rebuild the vec0 table (clears embeddings).
pub fn apply_embed_dimension_rebuild(conn: &Connection, dimension: i64) -> AppResult<AppSettings> {
    store::rebuild_embeddings_table(conn, dimension)?;
    get_settings(conn)
}

pub fn set_github_username(conn: &Connection, username: &str) -> AppResult<()> {
    set_value(conn, KEY_GITHUB_USERNAME, username)
}

pub fn clear_github_username(conn: &Connection) -> AppResult<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", [KEY_GITHUB_USERNAME])?;
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
        assert_eq!(settings.embed_dimension, 768);
        assert!(!settings.embeddings_need_rebuild);
        assert!(settings.github_username.is_none());

        let updated = update_settings(
            &conn,
            UpdateSettingsRequest {
                ollama_base_url: Some("http://192.168.1.10:11434".into()),
                ollama_chat_model: Some("qwen3:14b".into()),
                ollama_embed_model: None,
                embed_dimension: Some(1024),
            },
        )
        .expect("update");

        assert_eq!(updated.ollama_base_url, "http://192.168.1.10:11434");
        assert_eq!(updated.ollama_chat_model, "qwen3:14b");
        assert_eq!(updated.ollama_embed_model, "nomic-embed-text");
        assert_eq!(updated.embed_dimension, 1024);
        assert!(updated.embeddings_need_rebuild);
    }

    #[test]
    fn apply_embed_dimension_rebuild_clears_need_flag() {
        let conn = test_conn();
        update_settings(
            &conn,
            UpdateSettingsRequest {
                ollama_base_url: None,
                ollama_chat_model: None,
                ollama_embed_model: None,
                embed_dimension: Some(384),
            },
        )
        .expect("update");
        assert!(get_settings(&conn).expect("get").embeddings_need_rebuild);

        let after = apply_embed_dimension_rebuild(&conn, 384).expect("rebuild");
        assert_eq!(after.embed_dimension, 384);
        assert!(!after.embeddings_need_rebuild);
    }

    #[test]
    fn ollama_url_accepts_http_https_and_lan() {
        assert_eq!(
            validate_ollama_base_url("http://192.168.1.10:11434/").expect("lan"),
            "http://192.168.1.10:11434"
        );
        assert!(validate_ollama_base_url("https://ollama.local:11434").is_ok());
    }

    #[test]
    fn ollama_url_rejects_credentials_and_non_http() {
        let creds = validate_ollama_base_url("http://user:pass@127.0.0.1:11434").unwrap_err();
        assert!(creds.message.contains("credentials"));
        let file = validate_ollama_base_url("file:///tmp/ollama").unwrap_err();
        assert!(file.message.contains("http"));
        let empty = validate_ollama_base_url("   ").unwrap_err();
        assert!(empty.message.contains("empty"));
    }
}
