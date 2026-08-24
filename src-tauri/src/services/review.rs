use std::cell::RefCell;

use rusqlite::{params, Connection};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use crate::error::{AppError, AppResult};
use crate::models::{RepoFilters, RepoSummary, ReviewCounts, ReviewPreset, SetRepoReviewRequest};

/// No push (or never pushed) for this many months → Inactive queue.
pub const INACTIVE_MONTHS: i64 = 12;
/// Starred this many months ago (and stale push) → Possibly forgotten.
pub const FORGOTTEN_STARRED_MONTHS: i64 = 24;
/// Last push older than this many months (or never) → Possibly forgotten.
pub const FORGOTTEN_PUSHED_MONTHS: i64 = 18;
/// Fixed snooze lengths (days). No custom durations.
pub const SNOOZE_DAYS: [i64; 4] = [7, 30, 90, 180];

/// Frozen clock used by review unit tests (`2026-08-24T12:00:00Z`).
#[cfg(test)]
pub const TEST_NOW: &str = "2026-08-24T12:00:00Z";

thread_local! {
    static NOW_OVERRIDE: RefCell<Option<String>> = const { RefCell::new(None) };
}

impl ReviewPreset {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Uncategorized => "Uncategorized",
            Self::Archived => "Archived, still starred",
            Self::Inactive => "No push in 12+ months",
            Self::Forgotten => "Starred 2+ years · no push in 18+ months",
            Self::Unstarred => "Unstarred",
        }
    }
}

/// Current review clock as RFC3339 UTC. Tests may freeze this via `with_frozen_now`.
pub fn review_now_iso() -> String {
    NOW_OVERRIDE.with(|cell| {
        if let Some(now) = cell.borrow().clone() {
            return now;
        }
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
    })
}

#[cfg(test)]
pub fn with_frozen_now<T>(iso: &str, f: impl FnOnce() -> T) -> T {
    NOW_OVERRIDE.with(|cell| {
        *cell.borrow_mut() = Some(iso.to_string());
    });
    let out = f();
    NOW_OVERRIDE.with(|cell| {
        *cell.borrow_mut() = None;
    });
    out
}

/// Append review-preset membership, visibility overrides, and reviewed/snooze exclusion.
pub fn append_review_clauses(
    filters: &RepoFilters,
    now: &str,
    clauses: &mut Vec<String>,
    bind: &mut Vec<String>,
) {
    let Some(preset) = filters.review_preset.as_ref() else {
        return;
    };

    match preset {
        ReviewPreset::Uncategorized => {
            clauses.push(uncategorized_sql().to_string());
        }
        ReviewPreset::Archived => {
            clauses.push("r.archived = 1".into());
            clauses.push("r.unstarred = 0".into());
        }
        ReviewPreset::Inactive => {
            clauses.push(inactive_sql());
            bind.push(now.to_string());
        }
        ReviewPreset::Forgotten => {
            clauses.push(forgotten_sql());
            bind.push(now.to_string());
            bind.push(now.to_string());
        }
        ReviewPreset::Unstarred => {
            clauses.push("r.unstarred = 1".into());
        }
    }

    // Reviewed rows stay out of every queue until cleared.
    clauses.push(
        "NOT EXISTS (
            SELECT 1 FROM repo_review rv
            WHERE rv.repo_id = r.id AND rv.reviewed_at IS NOT NULL
         )"
        .into(),
    );
    // Active snooze hides the row; expired snooze does not.
    clauses.push(
        "NOT EXISTS (
            SELECT 1 FROM repo_review rv
            WHERE rv.repo_id = r.id
              AND rv.snoozed_until IS NOT NULL
              AND datetime(rv.snoozed_until) > datetime(?)
         )"
        .into(),
    );
    bind.push(now.to_string());
}

/// Whether default hide-unstarred should apply given the active preset.
pub fn effective_hide_unstarred(filters: &RepoFilters) -> bool {
    match filters.review_preset {
        Some(ReviewPreset::Unstarred) => false,
        Some(_) => true,
        None => filters.hide_unstarred.unwrap_or(true),
    }
}

/// Whether default hide-archived should apply given the active preset.
pub fn effective_hide_archived(filters: &RepoFilters) -> bool {
    match filters.review_preset {
        Some(ReviewPreset::Archived) => false,
        Some(ReviewPreset::Unstarred) => false,
        None => filters.hide_archived.unwrap_or(true),
        Some(_) => filters.hide_archived.unwrap_or(true),
    }
}

pub fn attach_review_reason(items: &mut [RepoSummary], preset: Option<&ReviewPreset>) {
    let Some(preset) = preset.copied() else {
        return;
    };
    let reason = preset.reason();
    for item in items {
        item.review_reason = Some(reason.to_string());
    }
}

pub fn load_review_state(
    conn: &Connection,
    repo_id: i64,
) -> AppResult<(Option<String>, Option<String>)> {
    let mut stmt =
        conn.prepare("SELECT reviewed_at, snoozed_until FROM repo_review WHERE repo_id = ?1")?;
    let mut rows = stmt.query([repo_id])?;
    if let Some(row) = rows.next()? {
        Ok((row.get(0)?, row.get(1)?))
    } else {
        Ok((None, None))
    }
}

pub fn review_counts(conn: &Connection) -> AppResult<ReviewCounts> {
    review_counts_at(conn, &review_now_iso())
}

pub fn review_counts_at(conn: &Connection, now: &str) -> AppResult<ReviewCounts> {
    Ok(ReviewCounts {
        uncategorized: count_preset(conn, ReviewPreset::Uncategorized, now)?,
        archived: count_preset(conn, ReviewPreset::Archived, now)?,
        inactive: count_preset(conn, ReviewPreset::Inactive, now)?,
        forgotten: count_preset(conn, ReviewPreset::Forgotten, now)?,
        unstarred: count_preset(conn, ReviewPreset::Unstarred, now)?,
        active_stars: conn.query_row(
            "SELECT COUNT(*) FROM repos WHERE unstarred = 0",
            [],
            |row| row.get(0),
        )?,
        oldest_starred_at: conn
            .query_row(
                "SELECT MIN(starred_at) FROM repos WHERE unstarred = 0",
                [],
                |row| row.get(0),
            )
            .optional_min()?,
    })
}

pub fn set_repo_review(conn: &Connection, request: &SetRepoReviewRequest) -> AppResult<()> {
    set_repo_review_at(conn, request, &review_now_iso())
}

pub fn set_repo_review_at(
    conn: &Connection,
    request: &SetRepoReviewRequest,
    now: &str,
) -> AppResult<()> {
    if request.repo_id <= 0 {
        return Err(AppError::new("validation_error", "repo_id is required"));
    }
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM repos WHERE id = ?1)",
        [request.repo_id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(AppError::new(
            "not_found",
            format!("repo {} not found", request.repo_id),
        ));
    }

    if let Some(days) = request.snooze_days {
        if days != 0 && !SNOOZE_DAYS.contains(&days) {
            return Err(AppError::new(
                "validation_error",
                format!("snooze_days must be one of {SNOOZE_DAYS:?} or 0"),
            ));
        }
    }

    let (cur_reviewed, cur_snooze) = load_review_state(conn, request.repo_id)?;

    let reviewed_at = match request.reviewed {
        Some(true) => Some(now.to_string()),
        Some(false) => None,
        None => cur_reviewed,
    };

    let snoozed_until = if request.reviewed == Some(true) {
        None
    } else if let Some(days) = request.snooze_days {
        if days == 0 {
            None
        } else {
            Some(add_days(now, days)?)
        }
    } else {
        cur_snooze
    };

    conn.execute(
        "INSERT INTO repo_review (repo_id, reviewed_at, snoozed_until)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(repo_id) DO UPDATE SET
            reviewed_at = excluded.reviewed_at,
            snoozed_until = excluded.snoozed_until",
        params![request.repo_id, reviewed_at, snoozed_until],
    )?;
    Ok(())
}

fn count_preset(conn: &Connection, preset: ReviewPreset, now: &str) -> AppResult<i64> {
    let filters = RepoFilters {
        review_preset: Some(preset),
        hide_unstarred: Some(true),
        hide_archived: Some(preset != ReviewPreset::Archived && preset != ReviewPreset::Unstarred),
        ..Default::default()
    };
    let (where_sql, bind) = crate::services::repos::build_filter_clause_at(&filters, None, now)?;
    let sql = format!("SELECT COUNT(*) FROM repos r {where_sql}");
    let mut stmt = conn.prepare(&sql)?;
    Ok(stmt.query_row(rusqlite::params_from_iter(bind.iter()), |row| row.get(0))?)
}

fn uncategorized_sql() -> &'static str {
    "NOT EXISTS (
        SELECT 1 FROM repo_categories rc
        JOIN categories c ON c.id = rc.category_id
        WHERE rc.repo_id = r.id
          AND LOWER(TRIM(c.name)) != 'uncategorized'
     )"
}

fn inactive_sql() -> String {
    format!(
        "(r.pushed_at IS NULL OR datetime(r.pushed_at) < datetime(?, '-{INACTIVE_MONTHS} months'))"
    )
}

fn forgotten_sql() -> String {
    format!(
        "datetime(r.starred_at) < datetime(?, '-{FORGOTTEN_STARRED_MONTHS} months')
         AND (r.pushed_at IS NULL OR datetime(r.pushed_at) < datetime(?, '-{FORGOTTEN_PUSHED_MONTHS} months'))"
    )
}

fn add_days(now: &str, days: i64) -> AppResult<String> {
    let parsed = OffsetDateTime::parse(now, &Rfc3339)
        .map_err(|e| AppError::new("validation_error", format!("invalid review clock: {e}")))?;
    parsed
        .checked_add(Duration::days(days))
        .ok_or_else(|| AppError::new("validation_error", "snooze overflow"))?
        .format(&Rfc3339)
        .map_err(|e| AppError::new("validation_error", format!("format snooze: {e}")))
}

trait OptionalMin {
    fn optional_min(self) -> AppResult<Option<String>>;
}

impl OptionalMin for rusqlite::Result<Option<String>> {
    fn optional_min(self) -> AppResult<Option<String>> {
        Ok(self?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ListReposRequest, RepoSort, SearchReposRequest, StarredRepo};
    use crate::services::repos::list_repos;
    use crate::services::search::search_repos;
    use crate::services::store::open_and_migrate;
    use crate::services::sync::apply_full_diff;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_conn() -> Connection {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("starboard_review_{nanos}.db"));
        open_and_migrate(&path).expect("migrate")
    }

    fn repo(
        id: i64,
        name: &str,
        lang: &str,
        starred_at: &str,
        pushed_at: Option<&str>,
        archived: bool,
    ) -> StarredRepo {
        StarredRepo {
            id,
            full_name: format!("owner/{name}"),
            owner: "owner".into(),
            name: name.into(),
            description: Some(format!("{name} {lang} toolkit")),
            language: Some(lang.into()),
            topics: format!("[\"{lang}\",\"stars\"]"),
            stars_count: Some(id * 10),
            forks_count: Some(1),
            open_issues: Some(0),
            license: Some("MIT".into()),
            homepage: None,
            html_url: format!("https://github.com/owner/{name}"),
            archived,
            fork: false,
            repo_created_at: Some("2020-01-01T00:00:00Z".into()),
            pushed_at: pushed_at.map(str::to_string),
            starred_at: starred_at.into(),
        }
    }

    fn seed(conn: &Connection) {
        let rows = [
            repo(
                1,
                "fresh-cat",
                "Rust",
                "2026-07-01T00:00:00Z",
                Some("2026-08-01T00:00:00Z"),
                false,
            ),
            repo(
                2,
                "uncat-active",
                "Rust",
                "2026-06-01T00:00:00Z",
                Some("2026-07-01T00:00:00Z"),
                false,
            ),
            repo(
                3,
                "uncat-bucket",
                "Go",
                "2026-05-01T00:00:00Z",
                Some("2026-06-01T00:00:00Z"),
                false,
            ),
            repo(
                4,
                "archived-star",
                "Rust",
                "2025-01-01T00:00:00Z",
                Some("2023-01-01T00:00:00Z"),
                true,
            ),
            repo(
                5,
                "inactive-only",
                "Rust",
                "2026-01-01T00:00:00Z",
                Some("2025-07-01T00:00:00Z"),
                false,
            ),
            repo(
                6,
                "forgotten",
                "Python",
                "2023-01-01T00:00:00Z",
                Some("2024-01-01T00:00:00Z"),
                false,
            ),
            repo(
                7,
                "forgotten-null",
                "Rust",
                "2023-01-01T00:00:00Z",
                None,
                false,
            ),
            repo(
                8,
                "unstarred-hist",
                "Rust",
                "2022-01-01T00:00:00Z",
                Some("2022-01-01T00:00:00Z"),
                false,
            ),
            repo(
                9,
                "unstarred-arch",
                "Go",
                "2022-01-01T00:00:00Z",
                Some("2022-01-01T00:00:00Z"),
                true,
            ),
            repo(
                10,
                "reviewed-uncat",
                "Rust",
                "2026-04-01T00:00:00Z",
                Some("2026-05-01T00:00:00Z"),
                false,
            ),
            repo(
                11,
                "snoozed-uncat",
                "Rust",
                "2026-04-15T00:00:00Z",
                Some("2026-05-15T00:00:00Z"),
                false,
            ),
            repo(
                12,
                "expired-snooze",
                "Go",
                "2026-03-01T00:00:00Z",
                Some("2026-04-01T00:00:00Z"),
                false,
            ),
            repo(
                13,
                "inactive-older",
                "Rust",
                "2026-02-01T00:00:00Z",
                Some("2024-01-01T00:00:00Z"),
                false,
            ),
            repo(
                14,
                "rust-forgotten",
                "Rust",
                "2023-06-01T00:00:00Z",
                Some("2024-06-01T00:00:00Z"),
                false,
            ),
            repo(
                15,
                "go-forgotten",
                "Go",
                "2023-06-01T00:00:00Z",
                Some("2024-06-01T00:00:00Z"),
                false,
            ),
        ];
        apply_full_diff(conn, &rows).expect("seed");

        // Soft-delete history rows (still locally stored).
        conn.execute("UPDATE repos SET unstarred = 1 WHERE id IN (8, 9)", [])
            .expect("unstar");

        conn.execute(
            "INSERT INTO categories (id, name, parent_id) VALUES
                (1, 'AI', NULL),
                (2, 'Uncategorized', NULL)",
            [],
        )
        .expect("categories");
        conn.execute(
            "INSERT INTO repo_categories (repo_id, category_id, source, confidence) VALUES
                (1, 1, 'llm', 0.9),
                (3, 2, 'llm', 0.4),
                (4, 1, 'manual', 1.0),
                (5, 1, 'llm', 0.8),
                (6, 1, 'llm', 0.8),
                (7, 1, 'llm', 0.8),
                (8, 1, 'llm', 0.8),
                (13, 1, 'llm', 0.8),
                (14, 1, 'llm', 0.8),
                (15, 1, 'llm', 0.8)",
            [],
        )
        .expect("assignments");

        conn.execute(
            "INSERT INTO repo_review (repo_id, reviewed_at, snoozed_until) VALUES
                (10, '2026-08-01T00:00:00Z', NULL),
                (11, NULL, '2026-09-01T00:00:00Z'),
                (12, NULL, '2026-08-01T00:00:00Z')",
            [],
        )
        .expect("review state");
    }

    fn names(result: &crate::models::RepoListResult) -> Vec<String> {
        result
            .items
            .iter()
            .map(|r| r.full_name.rsplit('/').next().unwrap_or("").to_string())
            .collect()
    }

    fn list_preset(conn: &Connection, preset: ReviewPreset) -> crate::models::RepoListResult {
        list_preset_filters(conn, preset, RepoFilters::default())
    }

    fn list_preset_filters(
        conn: &Connection,
        preset: ReviewPreset,
        mut filters: RepoFilters,
    ) -> crate::models::RepoListResult {
        filters.review_preset = Some(preset);
        list_repos(
            conn,
            ListReposRequest {
                filters: Some(filters),
                sort: Some(RepoSort::Stale),
                sort_desc: Some(false),
                limit: None,
                offset: None,
            },
        )
        .expect("list")
    }

    #[test]
    fn preset_membership_and_active_unstarred_semantics() {
        let conn = test_conn();
        seed(&conn);

        with_frozen_now(TEST_NOW, || {
            let uncat = list_preset(&conn, ReviewPreset::Uncategorized);
            let uncat_names = names(&uncat);
            assert!(uncat_names.contains(&"uncat-active".into()));
            assert!(
                uncat_names.contains(&"uncat-bucket".into()),
                "LLM Uncategorized bucket still counts as uncategorized: {uncat_names:?}"
            );
            assert!(uncat_names.contains(&"expired-snooze".into()));
            assert!(
                !uncat_names.contains(&"reviewed-uncat".into()),
                "reviewed must be excluded"
            );
            assert!(
                !uncat_names.contains(&"snoozed-uncat".into()),
                "active snooze must be excluded"
            );
            assert!(
                !uncat_names.contains(&"fresh-cat".into()),
                "real category is not uncategorized"
            );
            assert!(
                !uncat_names.contains(&"unstarred-hist".into()),
                "unstarred excluded from active uncategorized"
            );
            assert!(uncat
                .items
                .iter()
                .all(|r| r.review_reason.as_deref() == Some("Uncategorized")));

            let archived = list_preset(&conn, ReviewPreset::Archived);
            let archived_names = names(&archived);
            assert_eq!(archived_names, vec!["archived-star".to_string()]);
            assert!(
                !archived_names.contains(&"unstarred-arch".into()),
                "unstarred archived is history, not the archived queue"
            );

            let inactive = list_preset(&conn, ReviewPreset::Inactive);
            let inactive_names = names(&inactive);
            assert!(inactive_names.contains(&"inactive-only".into()));
            assert!(inactive_names.contains(&"forgotten".into()));
            assert!(inactive_names.contains(&"forgotten-null".into()));
            assert!(inactive_names.contains(&"inactive-older".into()));
            assert!(!inactive_names.contains(&"fresh-cat".into()));
            assert!(
                !inactive_names.contains(&"archived-star".into()),
                "archived hidden by default even when inactive"
            );
            assert!(!inactive_names.contains(&"unstarred-hist".into()));

            let forgotten = list_preset(&conn, ReviewPreset::Forgotten);
            let forgotten_names = names(&forgotten);
            assert!(forgotten_names.contains(&"forgotten".into()));
            assert!(forgotten_names.contains(&"forgotten-null".into()));
            assert!(forgotten_names.contains(&"rust-forgotten".into()));
            assert!(forgotten_names.contains(&"go-forgotten".into()));
            assert!(
                !forgotten_names.contains(&"inactive-only".into()),
                "starred recently → not forgotten"
            );
            assert!(!forgotten_names.contains(&"unstarred-hist".into()));

            let history = list_preset(&conn, ReviewPreset::Unstarred);
            let history_names = names(&history);
            assert!(history_names.contains(&"unstarred-hist".into()));
            assert!(history_names.contains(&"unstarred-arch".into()));
            assert!(!history_names.contains(&"fresh-cat".into()));
            assert!(history.items.iter().all(|r| r.unstarred));
        });
    }

    #[test]
    fn queue_orders_oldest_most_stale_first() {
        let conn = test_conn();
        seed(&conn);

        with_frozen_now(TEST_NOW, || {
            let inactive = list_preset(&conn, ReviewPreset::Inactive);
            let names = names(&inactive);
            let null_pos = names
                .iter()
                .position(|n| n == "forgotten-null")
                .expect("null");
            assert_eq!(null_pos, 0, "null pushed_at is the most stale: {names:?}");

            let older = names
                .iter()
                .position(|n| n == "inactive-older")
                .expect("older");
            let newer = names
                .iter()
                .position(|n| n == "inactive-only")
                .expect("newer");
            assert!(
                older < newer,
                "older push should rank before newer push: {names:?}"
            );

            let forgotten = list_preset(&conn, ReviewPreset::Forgotten);
            assert_eq!(
                forgotten.items[0].review_reason.as_deref(),
                Some("Starred 2+ years · no push in 18+ months")
            );
        });
    }

    #[test]
    fn pagination_is_stable_across_offsets() {
        let conn = test_conn();
        seed(&conn);

        with_frozen_now(TEST_NOW, || {
            let page1 = list_repos(
                &conn,
                ListReposRequest {
                    filters: Some(RepoFilters {
                        review_preset: Some(ReviewPreset::Inactive),
                        ..Default::default()
                    }),
                    sort: Some(RepoSort::Stale),
                    sort_desc: Some(false),
                    limit: Some(2),
                    offset: Some(0),
                },
            )
            .expect("page1");
            let page2 = list_repos(
                &conn,
                ListReposRequest {
                    filters: Some(RepoFilters {
                        review_preset: Some(ReviewPreset::Inactive),
                        ..Default::default()
                    }),
                    sort: Some(RepoSort::Stale),
                    sort_desc: Some(false),
                    limit: Some(2),
                    offset: Some(2),
                },
            )
            .expect("page2");
            assert_eq!(page1.items.len(), 2);
            assert_eq!(page2.items.len(), 2);
            let ids1: Vec<i64> = page1.items.iter().map(|r| r.id).collect();
            let ids2: Vec<i64> = page2.items.iter().map(|r| r.id).collect();
            assert!(
                ids1.iter().all(|id| !ids2.contains(id)),
                "pages must not overlap: {ids1:?} vs {ids2:?}"
            );
            assert_eq!(page1.total, page2.total);
            assert!(page1.total >= 4);
        });
    }

    #[test]
    fn reviewed_and_snooze_exclusion_and_expiry() {
        let conn = test_conn();
        seed(&conn);

        with_frozen_now(TEST_NOW, || {
            set_repo_review(
                &conn,
                &SetRepoReviewRequest {
                    repo_id: 2,
                    reviewed: Some(true),
                    snooze_days: None,
                },
            )
            .expect("review");

            let uncat = list_preset(&conn, ReviewPreset::Uncategorized);
            assert!(!names(&uncat).contains(&"uncat-active".into()));

            set_repo_review(
                &conn,
                &SetRepoReviewRequest {
                    repo_id: 2,
                    reviewed: Some(false),
                    snooze_days: Some(7),
                },
            )
            .expect("snooze");
            let still_hidden = list_preset(&conn, ReviewPreset::Uncategorized);
            assert!(!names(&still_hidden).contains(&"uncat-active".into()));
        });

        // Mid-snooze: still hidden.
        with_frozen_now("2026-08-28T12:00:00Z", || {
            let uncat = list_preset(&conn, ReviewPreset::Uncategorized);
            assert!(!names(&uncat).contains(&"uncat-active".into()));
        });

        // After 7 days from 2026-08-24: visible again.
        with_frozen_now("2026-09-01T12:00:00Z", || {
            let uncat = list_preset(&conn, ReviewPreset::Uncategorized);
            assert!(
                names(&uncat).contains(&"uncat-active".into()),
                "expired snooze should return to the queue"
            );
        });
    }

    #[test]
    fn invalid_snooze_rejected() {
        let conn = test_conn();
        seed(&conn);
        let err = set_repo_review(
            &conn,
            &SetRepoReviewRequest {
                repo_id: 2,
                reviewed: None,
                snooze_days: Some(14),
            },
        )
        .expect_err("14 days is not a fixed option");
        assert_eq!(err.code, "validation_error");
    }

    #[test]
    fn review_counts_exclude_unstarred_from_active_queues() {
        let conn = test_conn();
        seed(&conn);

        with_frozen_now(TEST_NOW, || {
            let counts = review_counts(&conn).expect("counts");
            assert!(counts.active_stars >= 1);
            assert_eq!(counts.archived, 1, "only currently starred archived");
            assert_eq!(counts.unstarred, 2);
            assert!(
                counts.uncategorized >= 2,
                "uncat-active + uncat-bucket + expired-snooze: {}",
                counts.uncategorized
            );
            assert!(counts.inactive >= 3);
            assert!(counts.forgotten >= 3);
            assert!(counts.oldest_starred_at.is_some());
        });
    }

    #[test]
    fn composes_with_language_and_keyword_search() {
        let conn = test_conn();
        seed(&conn);

        with_frozen_now(TEST_NOW, || {
            let rust_only = list_preset_filters(
                &conn,
                ReviewPreset::Forgotten,
                RepoFilters {
                    language: Some("Rust".into()),
                    ..Default::default()
                },
            );
            let rust_names = names(&rust_only);
            assert!(rust_names.contains(&"rust-forgotten".into()));
            assert!(rust_names.contains(&"forgotten-null".into()));
            assert!(!rust_names.contains(&"go-forgotten".into()));
            assert!(!rust_names.contains(&"forgotten".into())); // Python

            let searched = search_repos(
                &conn,
                SearchReposRequest {
                    query: "go-forgotten".into(),
                    filters: Some(RepoFilters {
                        review_preset: Some(ReviewPreset::Forgotten),
                        ..Default::default()
                    }),
                    sort: Some(RepoSort::Stale),
                    sort_desc: Some(false),
                    limit: None,
                    offset: None,
                    mode: None,
                },
            )
            .expect("search");
            assert_eq!(names(&searched), vec!["go-forgotten".to_string()]);
            assert_eq!(
                searched.items[0].review_reason.as_deref(),
                Some("Starred 2+ years · no push in 18+ months")
            );
        });
    }

    #[test]
    fn browse_without_preset_still_shows_reviewed() {
        let conn = test_conn();
        seed(&conn);
        let result = list_repos(
            &conn,
            ListReposRequest {
                filters: Some(RepoFilters {
                    hide_unstarred: Some(true),
                    hide_archived: Some(true),
                    ..Default::default()
                }),
                sort: Some(RepoSort::Name),
                sort_desc: Some(false),
                limit: None,
                offset: None,
            },
        )
        .expect("browse");
        let n = names(&result);
        assert!(n.contains(&"reviewed-uncat".into()));
        assert!(n.contains(&"fresh-cat".into()));
        assert!(!n.contains(&"unstarred-hist".into()));
        assert!(result.items.iter().all(|r| r.review_reason.is_none()));
    }

    #[test]
    fn github_client_source_is_get_only() {
        let src = include_str!("github.rs");
        assert!(
            src.contains(".get(url)"),
            "starred/readme fetch must stay GET"
        );
        for verb in [".post(", ".put(", ".patch(", ".delete("] {
            assert!(
                !src.contains(verb),
                "review feature must not introduce GitHub write verb {verb}"
            );
        }
    }
}
