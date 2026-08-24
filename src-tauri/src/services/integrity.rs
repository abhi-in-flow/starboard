use rusqlite::Connection;
use time::OffsetDateTime;

use crate::error::AppResult;
use crate::models::IntegrityReport;

/// Run SQLite integrity + foreign-key checks. Safe to invoke from Settings;
/// does not modify data.
pub fn check_integrity(conn: &Connection) -> AppResult<IntegrityReport> {
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    let mut fk_stmt = conn.prepare("PRAGMA foreign_key_check")?;
    let fk_rows = fk_stmt.query_map([], |_| Ok(()))?;
    let mut foreign_key_violations = 0i64;
    for row in fk_rows {
        row?;
        foreign_key_violations += 1;
    }
    let checked_at = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into());
    let ok = integrity.eq_ignore_ascii_case("ok") && foreign_key_violations == 0;
    Ok(IntegrityReport {
        ok,
        integrity,
        foreign_key_violations,
        checked_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::store::open_and_migrate;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn fresh_db_passes_integrity_check() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("starboard_integrity_{nanos}.db"));
        let conn = open_and_migrate(&path).expect("migrate");
        let report = check_integrity(&conn).expect("check");
        assert!(report.ok);
        assert_eq!(report.integrity, "ok");
        assert_eq!(report.foreign_key_violations, 0);
        let _ = std::fs::remove_file(path);
    }
}
