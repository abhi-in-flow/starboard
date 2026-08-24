use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{backup::Backup, Connection, OpenFlags};
use time::OffsetDateTime;

use crate::error::{AppError, AppResult};
use crate::models::{
    BackupResult, BackupValidation, DataPanel, IntegrityCheckResult, RestoreResult,
};
use crate::services::settings;
use crate::services::store::{self, CURRENT_SCHEMA_VERSION, MIN_SUPPORTED_SCHEMA_VERSION};

pub const KEY_LAST_BACKUP_PATH: &str = "last_backup_path";
pub const KEY_LAST_BACKUP_AT: &str = "last_backup_at";
pub const KEY_LAST_BACKUP_OK: &str = "last_backup_ok";
pub const KEY_LAST_RESTORE_AT: &str = "last_restore_at";

const ALLOWED_EXTENSIONS: &[&str] = &["db", "sqlite", "sqlite3", "starboard-backup"];
const REQUIRED_TABLES: &[&str] = &[
    "repos",
    "categories",
    "repo_categories",
    "sync_log",
    "settings",
];

#[cfg(test)]
thread_local! {
    static FAIL_AFTER_REPLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Debug, Clone, Default)]
pub struct JobGuard {
    pub sync: bool,
    pub readme: bool,
    pub embed: bool,
    pub categorize: bool,
}

impl JobGuard {
    pub fn running_job(&self) -> Option<&'static str> {
        if self.sync {
            Some("sync")
        } else if self.readme {
            Some("README fetch")
        } else if self.embed {
            Some("embedding")
        } else if self.categorize {
            Some("categorization")
        } else {
            None
        }
    }
}

pub fn refuse_if_busy(jobs: &JobGuard) -> AppResult<()> {
    if let Some(name) = jobs.running_job() {
        return Err(AppError::new(
            "backup_busy",
            format!(
                "Cannot restore while {name} is running. Wait for that job to finish, then try again."
            ),
        ));
    }
    Ok(())
}

pub fn parse_user_db_path(raw: &str) -> AppResult<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(path_err("backup path is required"));
    }
    if trimmed.contains('\0') {
        return Err(path_err("backup path contains invalid characters"));
    }
    if trimmed.to_ascii_lowercase().starts_with("file:") {
        return Err(path_err("SQLite URI paths are not allowed"));
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err(path_err("backup path must be an absolute path"));
    }
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(path_err("backup path must not contain '..'"));
        }
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(path_err(
            "backup file must use a .db, .sqlite, .sqlite3, or .starboard-backup extension",
        ));
    }
    Ok(path)
}

pub fn validate_backup_dest_path(raw: &str, live_path: Option<&Path>) -> AppResult<PathBuf> {
    let path = parse_user_db_path(raw)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.is_dir() {
            return Err(path_err("backup directory does not exist"));
        }
    }
    reject_live_or_sidecar(&path, live_path)?;
    Ok(path)
}

pub fn validate_backup_source_path(raw: &str, live_path: Option<&Path>) -> AppResult<PathBuf> {
    let path = parse_user_db_path(raw)?;
    if !path.is_file() {
        return Err(path_err("backup file does not exist"));
    }
    reject_live_or_sidecar(&path, live_path)?;
    Ok(path)
}

fn reject_live_or_sidecar(path: &Path, live_path: Option<&Path>) -> AppResult<()> {
    let Some(live) = live_path else {
        return Ok(());
    };
    if same_path(path, live) {
        return Err(path_err(
            "cannot use the live Starboard database path as a backup file",
        ));
    }
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = sidecar_path(live, suffix);
        if same_path(path, &sidecar) {
            return Err(path_err(
                "cannot use a live database sidecar (-wal/-shm/-journal) as a backup file",
            ));
        }
    }
    Ok(())
}

fn sidecar_path(live: &Path, suffix: &str) -> PathBuf {
    let mut os = live.as_os_str().to_os_string();
    os.push(suffix);
    PathBuf::from(os)
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

fn path_err(message: impl Into<String>) -> AppError {
    AppError::new("backup_path_error", message)
}

fn backup_err(code: &'static str, message: impl Into<String>) -> AppError {
    AppError::new(code, message)
}

fn as_corrupt(err: AppError) -> AppError {
    if err.code == "db_error" {
        backup_err(
            "backup_corrupt",
            format!(
                "could not read backup as a SQLite database: {}",
                err.message
            ),
        )
    } else {
        err
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn copy_database(from: &Connection, to: &mut Connection) -> AppResult<()> {
    let backup = Backup::new(from, to)
        .map_err(|e| backup_err("backup_error", format!("SQLite backup API failed: {e}")))?;
    backup
        .run_to_completion(32, Duration::from_millis(10), None)
        .map_err(|e| backup_err("backup_error", format!("SQLite backup copy failed: {e}")))?;
    Ok(())
}

fn open_readonly(path: &Path) -> AppResult<Connection> {
    store::register_sqlite_vec();
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| {
        backup_err(
            "backup_corrupt",
            format!("could not open backup as a SQLite database: {e}"),
        )
    })
}

pub fn integrity_check(conn: &Connection) -> AppResult<IntegrityCheckResult> {
    let mut stmt = conn.prepare("PRAGMA integrity_check")?;
    let rows: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.is_empty() {
        return Ok(IntegrityCheckResult {
            ok: false,
            message: "integrity_check returned no rows".into(),
        });
    }
    let message = rows.join("; ");
    let ok = rows.len() == 1 && rows[0].eq_ignore_ascii_case("ok");
    Ok(IntegrityCheckResult { ok, message })
}

fn pragma_user_version(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

fn table_exists(conn: &Connection, name: &str) -> AppResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn missing_required_tables(conn: &Connection) -> AppResult<Vec<&'static str>> {
    let mut missing = Vec::new();
    for name in REQUIRED_TABLES {
        if !table_exists(conn, name)? {
            missing.push(*name);
        }
    }
    Ok(missing)
}

fn schema_version_of(conn: &Connection) -> AppResult<i64> {
    let version = pragma_user_version(conn)?;
    if version == 0 && missing_required_tables(conn)?.is_empty() {
        return Ok(1);
    }
    Ok(version)
}

fn count_table(conn: &Connection, table: &str) -> AppResult<i64> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    Ok(conn.query_row(&sql, [], |row| row.get(0))?)
}

fn inspect_starboard(conn: &Connection) -> AppResult<(i64, i64, i64)> {
    let integrity = integrity_check(conn)?;
    if !integrity.ok {
        return Err(backup_err(
            "backup_corrupt",
            format!(
                "backup failed PRAGMA integrity_check: {}",
                integrity.message
            ),
        ));
    }

    let missing = missing_required_tables(conn)?;
    if !missing.is_empty() {
        return Err(backup_err(
            "backup_unrelated",
            format!(
                "file is not a Starboard database (missing tables: {})",
                missing.join(", ")
            ),
        ));
    }

    let version = schema_version_of(conn)?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(backup_err(
            "backup_unsupported_schema",
            format!(
                "backup schema version {version} is newer than this app (supports {MIN_SUPPORTED_SCHEMA_VERSION}–{CURRENT_SCHEMA_VERSION}). Update Starboard before restoring."
            ),
        ));
    }
    if version < MIN_SUPPORTED_SCHEMA_VERSION {
        return Err(backup_err(
            "backup_unrelated",
            format!("backup schema version {version} is not a supported Starboard database"),
        ));
    }

    Ok((
        version,
        count_table(conn, "repos")?,
        count_table(conn, "categories")?,
    ))
}

pub fn validate_backup_file(path: &Path) -> AppResult<BackupValidation> {
    let conn = open_readonly(path)?;
    let (schema_version, repo_count, category_count) =
        inspect_starboard(&conn).map_err(as_corrupt)?;
    Ok(BackupValidation {
        ok: true,
        path: path.to_string_lossy().into_owned(),
        schema_version,
        repo_count,
        category_count,
        message: format!(
            "Valid Starboard backup (schema {schema_version}, {repo_count} repos, {category_count} categories)"
        ),
    })
}

fn record_backup_result(conn: &Connection, path: &Path, ok: bool) -> AppResult<()> {
    settings::set_value(conn, KEY_LAST_BACKUP_PATH, &path.to_string_lossy())?;
    settings::set_value(conn, KEY_LAST_BACKUP_AT, &now_rfc3339())?;
    settings::set_value(conn, KEY_LAST_BACKUP_OK, if ok { "true" } else { "false" })?;
    Ok(())
}

pub fn create_backup(
    live: &Connection,
    dest: &Path,
    live_path: Option<&Path>,
) -> AppResult<BackupResult> {
    reject_live_or_sidecar(dest, live_path)?;
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() && !parent.is_dir() {
            return Err(path_err("backup directory does not exist"));
        }
    }

    let tmp = dest.with_file_name(format!(
        "{}.partial",
        dest.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "starboard.db".into())
    ));
    let _ = std::fs::remove_file(&tmp);

    let copy_result = (|| -> AppResult<()> {
        let mut dest_conn = Connection::open(&tmp).map_err(|e| {
            backup_err("backup_error", format!("failed to create backup file: {e}"))
        })?;
        copy_database(live, &mut dest_conn)?;
        drop(dest_conn);
        let _ = validate_backup_file(&tmp)?;
        Ok(())
    })();

    if let Err(e) = copy_result {
        let _ = std::fs::remove_file(&tmp);
        let _ = record_backup_result(live, dest, false);
        return Err(e);
    }

    if dest.exists() {
        std::fs::remove_file(dest).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            backup_err(
                "backup_error",
                format!("failed to replace backup file: {e}"),
            )
        })?;
    }
    std::fs::rename(&tmp, dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        backup_err(
            "backup_error",
            format!("failed to finalize backup file: {e}"),
        )
    })?;

    let validation = validate_backup_file(dest)?;
    record_backup_result(live, dest, true)?;
    Ok(BackupResult {
        path: dest.to_string_lossy().into_owned(),
        created_at: now_rfc3339(),
        schema_version: validation.schema_version,
        repo_count: validation.repo_count,
    })
}

fn pre_restore_path(live_path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let stem = live_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "starboard".into());
    live_path.with_file_name(format!("{stem}.pre-restore.{stamp}.db"))
}

fn prepare_restored_version(conn: &Connection) -> AppResult<i64> {
    let version = schema_version_of(conn)?;
    if pragma_user_version(conn)? == 0 && missing_required_tables(conn)?.is_empty() {
        conn.pragma_update(None, "user_version", 1)?;
        return Ok(1);
    }
    Ok(version)
}

fn rollback_live(live: &mut Connection, snapshot: &Path) -> AppResult<()> {
    let src = Connection::open(snapshot).map_err(|e| {
        backup_err(
            "backup_error",
            format!(
                "restore failed and the safety copy could not be reopened ({}): {e}",
                snapshot.display()
            ),
        )
    })?;
    copy_database(&src, live)?;
    store::migrate_conn(live)?;
    let integrity = integrity_check(live)?;
    if !integrity.ok {
        return Err(backup_err(
            "backup_error",
            format!(
                "restore failed and the safety copy at {} also failed integrity_check: {}",
                snapshot.display(),
                integrity.message
            ),
        ));
    }
    Ok(())
}

pub fn restore_backup(
    live: &mut Connection,
    source: &Path,
    live_path: &Path,
    jobs: &JobGuard,
) -> AppResult<RestoreResult> {
    refuse_if_busy(jobs)?;
    reject_live_or_sidecar(source, Some(live_path))?;
    validate_backup_file(source)?;

    let snapshot = pre_restore_path(live_path);
    {
        let mut snap = Connection::open(&snapshot).map_err(|e| {
            backup_err(
                "backup_error",
                format!("failed to create pre-restore safety copy: {e}"),
            )
        })?;
        copy_database(live, &mut snap)?;
    }
    if let Err(e) = validate_backup_file(&snapshot) {
        let _ = std::fs::remove_file(&snapshot);
        return Err(backup_err(
            "backup_error",
            format!("pre-restore safety copy was not valid: {}", e.message),
        ));
    }

    let apply = (|| -> AppResult<(i64, bool, i64)> {
        let src = open_readonly(source)?;
        copy_database(&src, live)?;
        drop(src);

        #[cfg(test)]
        if FAIL_AFTER_REPLACE.with(|f| f.replace(false)) {
            return Err(backup_err(
                "backup_error",
                "injected restore failure after live database replacement",
            ));
        }

        let before = prepare_restored_version(live)?;
        store::migrate_conn(live)?;
        let integrity = integrity_check(live)?;
        if !integrity.ok {
            return Err(backup_err(
                "backup_corrupt",
                format!(
                    "restored database failed PRAGMA integrity_check: {}",
                    integrity.message
                ),
            ));
        }
        let after = pragma_user_version(live)?;
        Ok((after, after > before, count_table(live, "repos")?))
    })();

    match apply {
        Ok((schema_version, migrated, repo_count)) => {
            settings::set_value(live, KEY_LAST_RESTORE_AT, &now_rfc3339())?;
            Ok(RestoreResult {
                path: source.to_string_lossy().into_owned(),
                restored_at: now_rfc3339(),
                schema_version,
                migrated,
                pre_restore_backup_path: snapshot.to_string_lossy().into_owned(),
                repo_count,
            })
        }
        Err(e) => {
            if let Err(rollback_err) = rollback_live(live, &snapshot) {
                return Err(backup_err(
                    "backup_error",
                    format!(
                        "{}; also failed to restore the safety copy at {}: {}",
                        e.message,
                        snapshot.display(),
                        rollback_err.message
                    ),
                ));
            }
            Err(e)
        }
    }
}

pub fn data_panel(conn: &Connection, db_path: &str) -> AppResult<DataPanel> {
    let last_backup_ok = match settings::get_value(conn, KEY_LAST_BACKUP_OK)? {
        Some(raw) if raw.eq_ignore_ascii_case("true") => Some(true),
        Some(raw) if raw.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    };
    Ok(DataPanel {
        db_path: db_path.to_string(),
        last_backup_path: settings::get_value(conn, KEY_LAST_BACKUP_PATH)?,
        last_backup_at: settings::get_value(conn, KEY_LAST_BACKUP_AT)?,
        last_backup_ok,
        last_restore_at: settings::get_value(conn, KEY_LAST_RESTORE_AT)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::store::open_and_migrate;
    use rusqlite_migration::{Migrations, M};

    const MIGRATION_001: &str = include_str!("../../migrations/001_init.sql");
    const SECRET_PAT: &str = "ghp_TESTPAT_NOT_A_REAL_TOKEN_backup_probe";

    fn unique(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("starboard_backup_{label}_{nanos}.db"))
    }

    fn seed_library(conn: &Connection) {
        conn.execute(
            "INSERT INTO repos (
                id, full_name, owner, name, description, language, topics,
                stars_count, forks_count, open_issues, license, homepage, html_url,
                archived, fork, repo_created_at, pushed_at, starred_at,
                readme_excerpt, fetched_at, unstarred
             ) VALUES (
                11, 'owner/alpha', 'owner', 'alpha', 'seeded repo', 'Rust', '[\"llm\"]',
                42, 1, 0, 'MIT', NULL, 'https://github.com/owner/alpha',
                0, 0, '2020-01-01T00:00:00Z', '2024-01-01T00:00:00Z', '2024-02-01T00:00:00Z',
                'readme body', '2024-03-01T00:00:00Z', 0
             )",
            [],
        )
        .expect("repo");
        conn.execute(
            "INSERT INTO categories (id, name, parent_id) VALUES (1, 'AI/LLM', NULL)",
            [],
        )
        .expect("cat");
        conn.execute(
            "INSERT INTO repo_categories (repo_id, category_id, source, confidence)
             VALUES (11, 1, 'manual', 1.0)",
            [],
        )
        .expect("assign");
        settings::set_value(conn, "ollama_base_url", "http://192.168.10.20:11434")
            .expect("settings");
        settings::set_value(conn, "github_username", "seed-user").expect("user");
    }

    fn assert_seeded(conn: &Connection) {
        let name: String = conn
            .query_row("SELECT full_name FROM repos WHERE id = 11", [], |row| {
                row.get(0)
            })
            .expect("repo");
        assert_eq!(name, "owner/alpha");
        let cat: String = conn
            .query_row("SELECT name FROM categories WHERE id = 1", [], |row| {
                row.get(0)
            })
            .expect("cat");
        assert_eq!(cat, "AI/LLM");
        let url = settings::get_value(conn, "ollama_base_url")
            .expect("get")
            .expect("url");
        assert_eq!(url, "http://192.168.10.20:11434");
        let pat_key: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key IN ('github_pat', 'pat', 'token')",
                [],
                |row| row.get(0),
            )
            .expect("pat keys");
        assert_eq!(pat_key, 0);
    }

    fn open_v1(path: &PathBuf) -> Connection {
        store::register_sqlite_vec();
        let mut conn = Connection::open(path).expect("open");
        conn.execute_batch("PRAGMA foreign_keys = ON;").expect("fk");
        Migrations::new(vec![M::up(MIGRATION_001)])
            .to_latest(&mut conn)
            .expect("001");
        conn
    }

    #[test]
    fn path_validation_rejects_relative_traversal_and_bad_extension() {
        assert_eq!(
            parse_user_db_path("relative.db").unwrap_err().code,
            "backup_path_error"
        );
        assert_eq!(
            parse_user_db_path("/tmp/../etc/passwd.db")
                .unwrap_err()
                .code,
            "backup_path_error"
        );
        assert_eq!(
            parse_user_db_path("/tmp/notes.txt").unwrap_err().code,
            "backup_path_error"
        );
        assert_eq!(
            parse_user_db_path("file:/tmp/x.db").unwrap_err().code,
            "backup_path_error"
        );
        assert!(parse_user_db_path("/tmp/starboard.sqlite3").is_ok());
        assert!(parse_user_db_path("/tmp/starboard.starboard-backup").is_ok());
    }

    #[test]
    fn dest_must_not_be_live_db() {
        let live = unique("live");
        std::fs::write(&live, b"x").expect("touch");
        let err = validate_backup_dest_path(live.to_str().expect("utf8"), Some(live.as_path()))
            .unwrap_err();
        assert_eq!(err.code, "backup_path_error");
        let _ = std::fs::remove_file(&live);
    }

    #[test]
    fn source_must_exist_and_use_allowed_extension() {
        let missing = unique("missing");
        let err = validate_backup_source_path(missing.to_str().expect("utf8"), None).unwrap_err();
        assert_eq!(err.code, "backup_path_error");
    }

    #[test]
    fn backup_contains_seeded_library_and_excludes_pat_bytes() {
        let live_path = unique("src");
        let dest = unique("dest");
        let conn = open_and_migrate(&live_path).expect("migrate");
        seed_library(&conn);

        let result = create_backup(&conn, &dest, Some(live_path.as_path())).expect("backup");
        assert_eq!(result.repo_count, 1);
        assert_eq!(result.schema_version, CURRENT_SCHEMA_VERSION);

        let bytes = std::fs::read(&dest).expect("read");
        assert!(
            !bytes
                .windows(SECRET_PAT.len())
                .any(|w| w == SECRET_PAT.as_bytes()),
            "backup file contained the planted PAT"
        );

        let backup = Connection::open(&dest).expect("open dest");
        assert_seeded(&backup);
        let last = settings::get_value(&conn, KEY_LAST_BACKUP_OK)
            .expect("last")
            .expect("ok");
        assert_eq!(last, "true");

        let _ = std::fs::remove_file(&live_path);
        let _ = std::fs::remove_file(&dest);
    }

    #[test]
    fn validate_accepts_good_backup_and_rejects_bad_files() {
        let live_path = unique("good-live");
        let dest = unique("good-dest");
        let conn = open_and_migrate(&live_path).expect("migrate");
        seed_library(&conn);
        create_backup(&conn, &dest, Some(live_path.as_path())).expect("backup");

        let ok = validate_backup_file(&dest).expect("valid");
        assert!(ok.ok);
        assert_eq!(ok.repo_count, 1);
        assert_eq!(ok.category_count, 1);

        let garbage = unique("garbage");
        std::fs::write(&garbage, b"this is not a sqlite database").expect("write");
        let corrupt = validate_backup_file(&garbage).unwrap_err();
        assert_eq!(corrupt.code, "backup_corrupt");

        let unrelated = unique("unrelated");
        {
            let other = Connection::open(&unrelated).expect("open");
            other
                .execute("CREATE TABLE widgets (id INTEGER PRIMARY KEY)", [])
                .expect("table");
        }
        let not_ours = validate_backup_file(&unrelated).unwrap_err();
        assert_eq!(not_ours.code, "backup_unrelated");

        let newer = unique("newer");
        std::fs::copy(&dest, &newer).expect("copy");
        {
            let newer_conn = Connection::open(&newer).expect("open");
            newer_conn
                .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 7)
                .expect("bump");
        }
        let unsupported = validate_backup_file(&newer).unwrap_err();
        assert_eq!(unsupported.code, "backup_unsupported_schema");

        let _ = std::fs::remove_file(&live_path);
        let _ = std::fs::remove_file(&dest);
        let _ = std::fs::remove_file(&garbage);
        let _ = std::fs::remove_file(&unrelated);
        let _ = std::fs::remove_file(&newer);
    }

    #[test]
    fn restore_round_trip_replaces_live_data() {
        let live_path = unique("restore-live");
        let backup_path = unique("restore-src");
        let mut live = open_and_migrate(&live_path).expect("live");
        seed_library(&live);
        create_backup(&live, &backup_path, Some(live_path.as_path())).expect("backup");

        live.execute("DELETE FROM repo_categories", [])
            .expect("clear assign");
        live.execute("DELETE FROM repos", []).expect("clear repos");
        live.execute(
            "INSERT INTO repos (
                id, full_name, owner, name, description, language, topics,
                stars_count, forks_count, open_issues, license, homepage, html_url,
                archived, fork, repo_created_at, pushed_at, starred_at,
                readme_excerpt, fetched_at, unstarred
             ) VALUES (
                99, 'owner/mutated', 'owner', 'mutated', 'changed', 'Go', '[]',
                1, 0, 0, NULL, NULL, 'https://github.com/owner/mutated',
                0, 0, '2020-01-01T00:00:00Z', '2024-01-01T00:00:00Z', '2024-02-01T00:00:00Z',
                NULL, '2024-03-01T00:00:00Z', 0
             )",
            [],
        )
        .expect("mutated");

        let result = restore_backup(&mut live, &backup_path, &live_path, &JobGuard::default())
            .expect("restore");
        assert!(!result.pre_restore_backup_path.is_empty());
        assert_eq!(result.repo_count, 1);
        assert_seeded(&live);
        let mutated: i64 = live
            .query_row(
                "SELECT COUNT(*) FROM repos WHERE full_name = 'owner/mutated'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(mutated, 0);
        assert!(Path::new(&result.pre_restore_backup_path).is_file());

        let _ = std::fs::remove_file(&live_path);
        let _ = std::fs::remove_file(&backup_path);
        let _ = std::fs::remove_file(&result.pre_restore_backup_path);
    }

    #[test]
    fn failed_restore_leaves_original_intact() {
        let live_path = unique("fail-live");
        let backup_path = unique("fail-src");
        let mut live = open_and_migrate(&live_path).expect("live");
        seed_library(&live);
        create_backup(&live, &backup_path, Some(live_path.as_path())).expect("backup");

        live.execute(
            "UPDATE repos SET full_name = 'owner/original-keep' WHERE id = 11",
            [],
        )
        .expect("mutate live");

        FAIL_AFTER_REPLACE.with(|f| f.set(true));
        let err =
            restore_backup(&mut live, &backup_path, &live_path, &JobGuard::default()).unwrap_err();
        assert_eq!(err.code, "backup_error");

        let name: String = live
            .query_row("SELECT full_name FROM repos WHERE id = 11", [], |row| {
                row.get(0)
            })
            .expect("still original");
        assert_eq!(name, "owner/original-keep");

        let garbage = unique("reject-live-src");
        std::fs::write(&garbage, b"nope").expect("write");
        let reject =
            restore_backup(&mut live, &garbage, &live_path, &JobGuard::default()).unwrap_err();
        assert_eq!(reject.code, "backup_corrupt");
        let still: String = live
            .query_row("SELECT full_name FROM repos WHERE id = 11", [], |row| {
                row.get(0)
            })
            .expect("still original after reject");
        assert_eq!(still, "owner/original-keep");

        let _ = std::fs::remove_file(&live_path);
        let _ = std::fs::remove_file(&backup_path);
        let _ = std::fs::remove_file(&garbage);
    }

    #[test]
    fn restore_refuses_while_jobs_run() {
        let err = refuse_if_busy(&JobGuard {
            sync: true,
            ..JobGuard::default()
        })
        .unwrap_err();
        assert_eq!(err.code, "backup_busy");
        assert!(err.message.contains("sync"));

        let err = refuse_if_busy(&JobGuard {
            readme: true,
            ..JobGuard::default()
        })
        .unwrap_err();
        assert!(err.message.contains("README"));

        let live_path = unique("busy-live");
        let backup_path = unique("busy-src");
        let mut live = open_and_migrate(&live_path).expect("live");
        seed_library(&live);
        create_backup(&live, &backup_path, Some(live_path.as_path())).expect("backup");
        let err = restore_backup(
            &mut live,
            &backup_path,
            &live_path,
            &JobGuard {
                embed: true,
                ..JobGuard::default()
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "backup_busy");
        assert_seeded(&live);

        let _ = std::fs::remove_file(&live_path);
        let _ = std::fs::remove_file(&backup_path);
    }

    #[test]
    fn restore_migrates_older_supported_schema() {
        let v1_path = unique("v1");
        let live_path = unique("v2-live");
        {
            let v1 = open_v1(&v1_path);
            seed_library(&v1);
            let has_meta: i64 = v1
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name = 'repo_embedding_meta'",
                    [],
                    |row| row.get(0),
                )
                .expect("meta");
            assert_eq!(has_meta, 0);
        }

        let ok = validate_backup_file(&v1_path).expect("v1 valid");
        assert_eq!(ok.schema_version, 1);

        let mut live = open_and_migrate(&live_path).expect("v2 live");
        live.execute(
            "INSERT INTO repos (
                id, full_name, owner, name, description, language, topics,
                stars_count, forks_count, open_issues, license, homepage, html_url,
                archived, fork, repo_created_at, pushed_at, starred_at,
                readme_excerpt, fetched_at, unstarred
             ) VALUES (
                1, 'owner/current', 'owner', 'current', 'now', 'Rust', '[]',
                1, 0, 0, NULL, NULL, 'https://github.com/owner/current',
                0, 0, '2020-01-01T00:00:00Z', '2024-01-01T00:00:00Z', '2024-02-01T00:00:00Z',
                NULL, '2024-03-01T00:00:00Z', 0
             )",
            [],
        )
        .expect("current");

        let result = restore_backup(&mut live, &v1_path, &live_path, &JobGuard::default())
            .expect("restore v1");
        assert!(result.migrated);
        assert_eq!(result.schema_version, CURRENT_SCHEMA_VERSION);
        assert_seeded(&live);
        let has_meta: i64 = live
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'repo_embedding_meta'",
                [],
                |row| row.get(0),
            )
            .expect("meta after");
        let has_vec: i64 = live
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'repo_embeddings'",
                [],
                |row| row.get(0),
            )
            .expect("vec after");
        assert_eq!(has_meta, 1);
        assert_eq!(has_vec, 1);

        let _ = std::fs::remove_file(&v1_path);
        let _ = std::fs::remove_file(&live_path);
        let _ = std::fs::remove_file(&result.pre_restore_backup_path);
    }

    #[test]
    fn integrity_check_ok_on_fresh_db() {
        let path = unique("integrity");
        let conn = open_and_migrate(&path).expect("migrate");
        let result = integrity_check(&conn).expect("check");
        assert!(result.ok);
        assert_eq!(result.message.to_ascii_lowercase(), "ok");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn data_panel_reads_last_backup_keys() {
        let path = unique("panel");
        let conn = open_and_migrate(&path).expect("migrate");
        let empty = data_panel(&conn, "/tmp/starboard.db").expect("empty");
        assert_eq!(empty.db_path, "/tmp/starboard.db");
        assert!(empty.last_backup_path.is_none());

        settings::set_value(&conn, KEY_LAST_BACKUP_PATH, "/tmp/out.db").expect("path");
        settings::set_value(&conn, KEY_LAST_BACKUP_AT, "2024-01-01T00:00:00Z").expect("at");
        settings::set_value(&conn, KEY_LAST_BACKUP_OK, "true").expect("ok");
        let filled = data_panel(&conn, "/tmp/starboard.db").expect("filled");
        assert_eq!(filled.last_backup_path.as_deref(), Some("/tmp/out.db"));
        assert_eq!(filled.last_backup_ok, Some(true));
        let _ = std::fs::remove_file(path);
    }
}
