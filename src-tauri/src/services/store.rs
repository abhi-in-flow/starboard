use std::path::PathBuf;
use std::sync::{Mutex, Once};

use rusqlite::{Connection, Transaction};
use rusqlite_migration::{Migrations, M};
use tauri::{AppHandle, Manager, State};
use time::OffsetDateTime;

use crate::error::{AppError, AppResult};

pub struct DbState(pub Mutex<Connection>);

const MIGRATION_001: &str = include_str!("../../migrations/001_init.sql");
const MIGRATION_002: &str = include_str!("../../migrations/002_embeddings.sql");
const MIGRATION_003: &str = include_str!("../../migrations/003_hardening.sql");

/// Desktop local-first durability:
/// - `foreign_keys=ON` enforces referential integrity
/// - WAL allows readers during a writer (UI stays responsive)
/// - `busy_timeout=5000` waits briefly instead of failing on SQLITE_BUSY
/// - `synchronous=NORMAL` is the usual WAL companion: fsync at checkpoints, not
///   every commit. FULL would add latency on every write; OFF risks corruption
///   after a crash. NORMAL + WAL is the standard single-user desktop tradeoff.
const CONNECTION_PRAGMAS: &str = "
    PRAGMA foreign_keys = ON;
    PRAGMA journal_mode = WAL;
    PRAGMA busy_timeout = 5000;
    PRAGMA synchronous = NORMAL;
";

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
            let init: SqliteAutoExt =
                std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ());
            rusqlite::ffi::sqlite3_auto_extension(Some(init));
        }
    });
}

pub fn open_and_migrate(db_path: &PathBuf) -> AppResult<Connection> {
    register_sqlite_vec();

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::db(format!("failed to create app data directory: {e}")))?;
    }

    let mut conn = Connection::open(db_path)?;
    configure_connection(&conn)?;

    let migrations = Migrations::new(vec![
        M::up(MIGRATION_001),
        M::up(MIGRATION_002),
        M::up(MIGRATION_003),
    ]);
    migrations.to_latest(&mut conn)?;

    // Ensure vec0 dimension matches the current settings value (handles rebuild after
    // a settings change that already cleared meta + dropped the table).
    ensure_embeddings_table(&conn)?;
    reconcile_orphaned_sync_logs(&conn)?;
    backfill_document_hashes(&conn)?;

    Ok(conn)
}

/// Apply durability/concurrency PRAGMAs on every open (fresh or upgraded).
pub fn configure_connection(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(CONNECTION_PRAGMAS)?;
    Ok(())
}

/// Run `f` inside a single SQLite transaction. Rollback on any error so
/// multi-step writes (ETag + upserts, taxonomy + assignments, embed + meta)
/// never commit halfway.
pub fn with_tx<T, F>(conn: &Connection, f: F) -> AppResult<T>
where
    F: FnOnce(&Transaction<'_>) -> AppResult<T>,
{
    let tx = conn.unchecked_transaction()?;
    match f(&tx) {
        Ok(value) => {
            tx.commit()?;
            Ok(value)
        }
        Err(err) => Err(err),
    }
}

/// Mark leftover `sync_log` rows left `running` after a crash so the UI
/// never shows a stale in-progress job from a previous process.
pub fn reconcile_orphaned_sync_logs(conn: &Connection) -> AppResult<i64> {
    let now = now_rfc3339();
    let n = conn.execute(
        "UPDATE sync_log SET
            status = 'error',
            finished_at = ?1,
            error = COALESCE(error, 'interrupted by app restart')
         WHERE status = 'running'",
        [now],
    )?;
    Ok(n as i64)
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

/// Persist SHA-256 document hashes for rows that predate migration 003 so
/// embed-status can be a cheap SQL join instead of hashing the whole library.
pub fn backfill_document_hashes(conn: &Connection) -> AppResult<i64> {
    let mut stmt = conn.prepare(
        "SELECT id, full_name, description, topics, readme_excerpt
         FROM repos
         WHERE document_hash IS NULL",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut pending = Vec::new();
    for row in rows {
        pending.push(row?);
    }
    if pending.is_empty() {
        return Ok(0);
    }

    let mut updated = 0i64;
    for chunk in pending.chunks(200) {
        with_tx(conn, |tx| {
            for (id, full_name, description, topics, readme) in chunk {
                let document = crate::services::embed::build_document(
                    full_name,
                    description.as_deref(),
                    topics,
                    readme.as_deref(),
                );
                let hash = crate::services::embed::content_hash(&document);
                tx.execute(
                    "UPDATE repos SET document_hash = ?1 WHERE id = ?2",
                    rusqlite::params![hash, id],
                )?;
                updated += 1;
            }
            Ok(())
        })?;
    }
    Ok(updated)
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
    with_tx(conn, |tx| {
        tx.execute_batch("DROP TABLE IF EXISTS repo_embeddings;")?;
        tx.execute_batch("DELETE FROM repo_embedding_meta;")?;
        let sql = format!(
            "CREATE VIRTUAL TABLE repo_embeddings USING vec0(
               repo_id INTEGER PRIMARY KEY,
               embedding float[{dimension}]
             )"
        );
        tx.execute_batch(&sql)?;
        crate::services::settings::set_value(
            tx,
            crate::services::settings::KEY_EMBED_DIMENSION,
            &dimension.to_string(),
        )?;
        crate::services::settings::set_value(
            tx,
            crate::services::settings::KEY_EMBEDDINGS_TABLE_DIMENSION,
            &dimension.to_string(),
        )?;
        Ok(())
    })
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

        let has_hash: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('repos') WHERE name = 'document_hash'",
                [],
                |row| row.get(0),
            )
            .expect("document_hash column");
        assert_eq!(has_hash, 1);

        let idx: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_repos_starred_at_active'",
                [],
                |row| row.get(0),
            )
            .expect("browse index");
        assert_eq!(idx, 1);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pragmas_are_set_on_open() {
        let path = temp_db_path("pragmas");
        let conn = open_and_migrate(&path).expect("migrate");
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .expect("fk");
        assert_eq!(fk, 1);
        let journal: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .expect("wal");
        assert_eq!(journal.to_ascii_lowercase(), "wal");
        let timeout: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .expect("busy");
        assert_eq!(timeout, 5000);
        let sync: i64 = conn
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .expect("sync");
        // NORMAL == 1
        assert_eq!(sync, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn orphaned_running_sync_logs_are_reconciled_on_open() {
        let path = temp_db_path("orphan");
        {
            let conn = open_and_migrate(&path).expect("migrate");
            conn.execute(
                "INSERT INTO sync_log (started_at, kind, status) VALUES ('2024-01-01T00:00:00Z', 'full', 'running')",
                [],
            )
            .expect("insert");
        }
        let conn = open_and_migrate(&path).expect("reopen");
        let (status, error): (String, Option<String>) = conn
            .query_row(
                "SELECT status, error FROM sync_log ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("row");
        assert_eq!(status, "error");
        assert!(
            error.as_deref().unwrap_or("").contains("interrupted"),
            "expected crash-recovery message, got {error:?}"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn with_tx_rolls_back_on_error() {
        let path = temp_db_path("tx");
        let conn = open_and_migrate(&path).expect("migrate");
        crate::services::settings::set_value(&conn, "starred_etag", "old").expect("etag");
        let err = with_tx(&conn, |tx| {
            crate::services::settings::set_value(tx, "starred_etag", "new")?;
            tx.execute(
                "INSERT INTO repos (id, full_name, owner, name, html_url, starred_at, fetched_at)
                 VALUES (1, 'o/r', 'o', 'r', 'https://x', '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
                [],
            )?;
            Err::<(), _>(AppError::db("injected apply failure"))
        })
        .expect_err("should fail");
        assert_eq!(err.message, "injected apply failure");
        let etag = crate::services::settings::get_value(&conn, "starred_etag")
            .expect("get")
            .expect("etag");
        assert_eq!(etag, "old");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM repos", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migration_002_upgrades_phase3_db() {
        register_sqlite_vec();
        let path = temp_db_path("upgrade");
        {
            let mut conn = Connection::open(&path).expect("open");
            configure_connection(&conn).expect("pragma");
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

        let has_hash: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('repos') WHERE name = 'document_hash'",
                [],
                |row| row.get(0),
            )
            .expect("003 column");
        assert_eq!(has_hash, 1, "001→002→003 upgrade must add document_hash");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migration_001_002_003_upgrade_path() {
        register_sqlite_vec();
        let path = temp_db_path("upgrade123");
        {
            let mut conn = Connection::open(&path).expect("open");
            configure_connection(&conn).expect("pragma");
            Migrations::new(vec![M::up(MIGRATION_001)])
                .to_latest(&mut conn)
                .expect("001");
            Migrations::new(vec![M::up(MIGRATION_001), M::up(MIGRATION_002)])
                .to_latest(&mut conn)
                .expect("002");
            let has_003: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('repos') WHERE name = 'readme_status'",
                    [],
                    |row| row.get(0),
                )
                .expect("pre-003");
            assert_eq!(has_003, 0);
        }
        let conn = open_and_migrate(&path).expect("003");
        let has_status: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('repos') WHERE name = 'readme_status'",
                [],
                |row| row.get(0),
            )
            .expect("status col");
        let has_attempts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('repos') WHERE name = 'readme_attempts'",
                [],
                |row| row.get(0),
            )
            .expect("attempts col");
        assert_eq!(has_status, 1);
        assert_eq!(has_attempts, 1);
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
