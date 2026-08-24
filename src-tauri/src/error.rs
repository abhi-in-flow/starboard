use serde::Serialize;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error, Serialize)]
#[error("{code}: {message}")]
pub struct AppError {
    pub code: String,
    pub message: String,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn db(message: impl Into<String>) -> Self {
        Self::new("db_error", message)
    }

    pub fn settings(message: impl Into<String>) -> Self {
        Self::new("settings_error", message)
    }

    pub fn auth(message: impl Into<String>) -> Self {
        Self::new("auth_error", message)
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::new("network_error", message)
    }

    pub fn keyring(message: impl Into<String>) -> Self {
        Self::new("keyring_error", message)
    }

    pub fn sync(message: impl Into<String>) -> Self {
        Self::new("sync_error", message)
    }

    pub fn ollama(message: impl Into<String>) -> Self {
        Self::new("ollama_error", message)
    }

    pub fn cancelled(message: impl Into<String>) -> Self {
        Self::new("cancelled", message)
    }

    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new("rate_limited", message)
    }

    pub fn is_cancelled(&self) -> bool {
        self.code == "cancelled"
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        Self::db(value.to_string())
    }
}

impl From<rusqlite_migration::Error> for AppError {
    fn from(value: rusqlite_migration::Error) -> Self {
        Self::db(value.to_string())
    }
}

impl From<keyring::Error> for AppError {
    fn from(value: keyring::Error) -> Self {
        Self::keyring(value.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(value: reqwest::Error) -> Self {
        Self::network(value.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(value: tauri::Error) -> Self {
        Self::db(value.to_string())
    }
}
