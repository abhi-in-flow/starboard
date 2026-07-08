use keyring::Entry;

use crate::error::{AppError, AppResult};

const SERVICE: &str = "starboard";
const ACCOUNT: &str = "github_pat";

fn entry() -> AppResult<Entry> {
    Entry::new(SERVICE, ACCOUNT).map_err(AppError::from)
}

pub fn store_pat(pat: &str) -> AppResult<()> {
    let trimmed = pat.trim();
    if trimmed.is_empty() {
        return Err(AppError::auth("PAT cannot be empty"));
    }
    entry()?.set_password(trimmed)?;
    Ok(())
}

pub fn get_pat() -> AppResult<Option<String>> {
    match entry()?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::from(e)),
    }
}

pub fn delete_pat() -> AppResult<()> {
    match entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::from(e)),
    }
}
