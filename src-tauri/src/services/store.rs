use std::path::PathBuf;
use std::sync::{Mutex, Once};

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, AppResult};

pub struct DbState(pub Mutex<Connection>);

const MIGRATION_001: &str = include_str!("../../migrations/001_init.sql");
const MIGRATION_002: &str = include_str!("../../migrations/002_embeddings.sql");

/// Default embedding dimension for `nomic-embed-text` (HLD §4.2).
pub const DEFAULT_EMBED_DIMENSION: i64 = 768;

/// Register sqlite-vec once per process so every Connection can create/query vec0 tables.
pub fn register_sqlite_vec() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: sqlite3_vec_init matches the SQLite auto-extension callback ABI.
        // Transmute is the documented integration path for the sqlite-vec crate.
        type SqliteAutoExt = unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *mut std::os::raw::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int;
        unsafe {
            let init: SqliteAutoExt = std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ());
            rusqlite::ffi::sqlite3_auto_extension(Some(init));
        }
    });
}

pub fn open_and_migrate(db_path: &PathBuf) -> AppResult<Connection> {
    register_sqlite_vec();

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::db(format!("failed to create app data directory: {e}"))
        })?;
    }

    let mut conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    let migrations = Migrations::new(vec![M::up(MIGRATION_001), M::up(MIGRATION_002)]);
    migrations.to_latest(&mut conn)?;

    // Ensure vec0 dimension matches the current settings value (handles rebuild after
    // a settings change that already cleared meta + dropped the table).
    ensure_embeddings_table(&conn)?;

    Ok(conn)
}

/// Create `repo_embeddings` with the dimension stored in settings if missing.
pub fn ensure_embeddings_table(conn: &Connection) -> AppResult<()> {
    let dimension = crate::services::settings::get_embed_dimension(conn)?;
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'repo_embeddings'",
        [],
        |row| row.get(0),
    )?;
    if exists {
        return Ok(());
    }
    let sql = format!(
        "CREATE VIRTUAL TABLE repo_embeddings USING vec0(
           repo_id INTEGER PRIMARY KEY,
           embedding float[{dimension}]
         )"
    );
    conn.execute_batch(&sql)?;
    crate::services::settings::set_value(
        conn,
        crate::services::settings::KEY_EMBEDDINGS_TABLE_DIMENSION,
        &dimension.to_string(),
    )?;
    Ok(())
}

/// Drop and recreate the vec0 table for a new dimension; clears embedding meta.
pub fn rebuild_embeddings_table(conn: &Connection, dimension: i64) -> AppResult<()> {
    if !(1..=8192).contains(&dimension) {
        return Err(AppError::settings(format!(
            "embed_dimension out of range: {dimension}"
        )));
    }
    conn.execute_batch("DROP TABLE IF EXISTS repo_embeddings;")?;
    conn.execute_batch("DELETE FROM repo_embedding_meta;")?;
    let sql = format!(
        "CREATE VIRTUAL TABLE repo_embeddings USING vec0(
           repo_id INTEGER PRIMARY KEY,
           embedding float[{dimension}]
         )"
    );
    conn.execute_batch(&sql)?;
    crate::services::settings::set_value(
        conn,
        crate::services::settings::KEY_EMBED_DIMENSION,
        &dimension.to_string(),
    )?;
    crate::services::settings::set_value(
        conn,
        crate::services::settings::KEY_EMBEDDINGS_TABLE_DIMENSION,
        &dimension.to_string(),
    )?;
    Ok(())
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
    use rusqlite_migration::{Migrations, M};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("starboard_{label}_{nanos}.db"))
    }

    #[test]
    fn migration_creates_expected_tables() {
        let path = temp_db_path("fresh");
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
            "repo_embedding_meta",
            "repo_embeddings",
        ] {
            assert!(
                tables.iter().any(|t| t == expected),
                "missing schema object: {expected}; got {tables:?}"
            );
        }

        let dim: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'embed_dimension'",
                [],
                |row| row.get(0),
            )
            .expect("embed_dimension");
        assert_eq!(dim, "768");

        // sqlite-vec is live: vec_version() should resolve.
        let version: String = conn
            .query_row("SELECT vec_version()", [], |row| row.get(0))
            .expect("vec_version");
        assert!(!version.is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migration_002_upgrades_phase3_db() {
        register_sqlite_vec();
        let path = temp_db_path("upgrade");
        {
            let mut conn = Connection::open(&path).expect("open");
            conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
            Migrations::new(vec![M::up(MIGRATION_001)])
                .to_latest(&mut conn)
                .expect("001 only");

            let has_meta: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name = 'repo_embedding_meta'",
                    [],
                    |row| row.get(0),
                )
                .expect("check");
            assert_eq!(has_meta, 0, "001-only DB must not have Phase 4 tables yet");
        }

        // Re-open via full migrator — applies 002 on top of 001.
        let conn = open_and_migrate(&path).expect("upgrade");
        let has_meta: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'repo_embedding_meta'",
                [],
                |row| row.get(0),
            )
            .expect("check meta");
        let has_vec: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'repo_embeddings'",
                [],
                |row| row.get(0),
            )
            .expect("check vec");
        assert_eq!(has_meta, 1);
        assert_eq!(has_vec, 1);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rebuild_embeddings_table_changes_dimension() {
        let path = temp_db_path("rebuild");
        let conn = open_and_migrate(&path).expect("migrate");
        rebuild_embeddings_table(&conn, 384).expect("rebuild");

        let dim: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'embed_dimension'",
                [],
                |row| row.get(0),
            )
            .expect("dim");
        assert_eq!(dim, "384");

        // Insert a 384-d vector to prove the schema matches.
        let embedding = vec![0.1_f32; 384];
        let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        conn.execute(
            "INSERT INTO repo_embeddings(repo_id, embedding) VALUES (1, ?1)",
            rusqlite::params![bytes],
        )
        .expect("insert vector");

        let _ = std::fs::remove_file(path);
    }
}
