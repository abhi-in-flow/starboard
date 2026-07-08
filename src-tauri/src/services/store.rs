use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, AppResult};

pub struct DbState(pub Mutex<Connection>);

const MIGRATION_001: &str = include_str!("../../migrations/001_init.sql");

pub fn open_and_migrate(db_path: &PathBuf) -> AppResult<Connection> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::db(format!("failed to create app data directory: {e}"))
        })?;
    }

    let mut conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    let migrations = Migrations::new(vec![M::up(MIGRATION_001)]);
    migrations.to_latest(&mut conn)?;

    Ok(conn)
}

pub fn db_path(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::db(format!("failed to resolve app data dir: {e}")))?;
    Ok(dir.join("starboard.db"))
}

pub fn with_conn<T, F>(state: &State<'_, DbState>, f: F) -> AppResult<T>
where
    F: FnOnce(&Connection) -> AppResult<T>,
{
    let conn = state
        .0
        .lock()
        .map_err(|_| AppError::db("database lock poisoned"))?;
    f(&conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("starboard_test_{nanos}.db"))
    }

    #[test]
    fn migration_creates_expected_tables() {
        let path = temp_db_path();
        let conn = open_and_migrate(&path).expect("migrate");

        let tables: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT name FROM sqlite_master WHERE type IN ('table','trigger') ORDER BY name",
                )
                .expect("prepare");
            stmt.query_map([], |row| row.get(0))
                .expect("query")
                .filter_map(Result::ok)
                .collect()
        };

        for expected in [
            "repos",
            "categories",
            "repo_categories",
            "sync_log",
            "settings",
            "repos_fts",
            "repos_ai",
            "repos_ad",
            "repos_au",
        ] {
            assert!(
                tables.iter().any(|t| t == expected),
                "missing schema object: {expected}; got {tables:?}"
            );
        }

        let _ = std::fs::remove_file(path);
    }
}
