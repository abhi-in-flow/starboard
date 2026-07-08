use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use futures::stream::{self, StreamExt};
use rusqlite::{params, Connection};
use tauri::{AppHandle, Emitter, Manager};
use time::OffsetDateTime;
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::models::{StarredRepo, SyncProgress, SyncResult, SyncStatus};
use crate::services::github::GitHubClient;
use crate::services::settings::{self, KEY_LAST_SYNCED_AT, KEY_STARRED_ETAG};
use crate::services::store::DbState;

/// Max in-flight README requests.
const README_CONCURRENCY: usize = 6;
/// Minimum spacing between README request starts (~6 req/s).
const README_MIN_INTERVAL: Duration = Duration::from_millis(167);

pub struct SyncState {
    pub running: AtomicBool,
    pub readme_running: AtomicBool,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            running: AtomicBool::new(false),
            readme_running: AtomicBool::new(false),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiffStats {
    pub added: i64,
    pub updated: i64,
    pub removed: i64,
}

/// Pure sync-diff helper: soft-deletes local ids missing from remote.
pub fn diff_unstarred(local_active_ids: &HashSet<i64>, remote_ids: &HashSet<i64>) -> Vec<i64> {
    local_active_ids
        .difference(remote_ids)
        .copied()
        .collect()
}

pub fn get_sync_status(
    conn: &Connection,
    running: bool,
    readme_running: bool,
) -> AppResult<SyncStatus> {
    let last_synced_at = settings::get_value(conn, KEY_LAST_SYNCED_AT)?;
    let last_result = latest_sync_result(conn)?;
    let pending_readmes = count_pending_readmes(conn)?;
    Ok(SyncStatus {
        running,
        readme_running,
        pending_readmes,
        last_synced_at,
        last_result,
    })
}

fn latest_sync_result(conn: &Connection) -> AppResult<Option<SyncResult>> {
    let mut stmt = conn.prepare(
        "SELECT kind, repos_added, repos_updated, repos_removed, status, error
         FROM sync_log
         WHERE kind IN ('full', 'incremental')
         ORDER BY id DESC
         LIMIT 1",
    )?;
    let mut rows = stmt.query([])?;
    if let Some(row) = rows.next()? {
        Ok(Some(SyncResult {
            kind: row.get(0)?,
            repos_added: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
            repos_updated: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            repos_removed: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            readmes_fetched: 0,
            status: row.get(4)?,
            error: row.get(5)?,
        }))
    } else {
        Ok(None)
    }
}

pub async fn run_sync(app: AppHandle, full: bool) -> AppResult<SyncResult> {
    let sync_state = app.state::<SyncState>();
    if sync_state
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::sync("a sync is already running"));
    }

    let result = run_sync_inner(&app, full).await;
    sync_state.running.store(false, Ordering::SeqCst);
    result
}

async fn run_sync_inner(app: &AppHandle, full: bool) -> AppResult<SyncResult> {
    let pat = crate::services::secrets::get_pat()?
        .ok_or_else(|| AppError::auth("GitHub PAT is not configured"))?;
    let client = GitHubClient::production(pat)?;

    let kind = if full { "full" } else { "incremental" };
    let started_at = now_rfc3339();
    let log_id = with_db(app, |conn| begin_sync_log(conn, kind, &started_at))?;

    emit_progress(
        app,
        progress(kind, 0, 0, "Fetching starred repositories…", None),
    );

    let sync_outcome = if full {
        full_sync(app, &client).await
    } else {
        incremental_sync(app, &client).await
    };

    match sync_outcome {
        Ok(stats) => {
            let finished = now_rfc3339();
            with_db(app, |conn| {
                settings::set_value(conn, KEY_LAST_SYNCED_AT, &finished)?;
                finish_sync_log(conn, log_id, &finished, &stats, "ok", None)
            })?;
            emit_progress(
                app,
                progress(kind, 1, 1, "Sync complete — fetching READMEs…", None),
            );

            // Emit pending README count before returning so the UI doesn't flash empty.
            let mut pending = 0i64;
            if let Ok(n) = with_db(app, count_pending_readmes) {
                pending = n;
            }
            if pending > 0 {
                emit_progress(
                    app,
                    progress(
                        "readme",
                        0,
                        pending as u32,
                        format!("Fetching {pending} README excerpts…"),
                        None,
                    ),
                );
            }

            spawn_readme_queue_if_needed(app.clone());

            Ok(SyncResult {
                kind: kind.into(),
                repos_added: stats.added,
                repos_updated: stats.updated,
                repos_removed: stats.removed,
                readmes_fetched: 0,
                status: "ok".into(),
                error: None,
            })
        }
        Err(e) => {
            let finished = now_rfc3339();
            let empty = DiffStats::default();
            let msg = e.message.clone();
            let _ = with_db(app, |conn| {
                finish_sync_log(conn, log_id, &finished, &empty, "error", Some(&msg))
            });
            Err(e)
        }
    }
}

/// Start the README queue if there is pending work and no queue is already running.
pub fn spawn_readme_queue_if_needed(app: AppHandle) {
    let pending = match with_db(&app, count_pending_readmes) {
        Ok(n) => n,
        Err(_) => return,
    };
    if pending == 0 {
        return;
    }

    let sync_state = app.state::<SyncState>();
    if sync_state
        .readme_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    tauri::async_runtime::spawn(async move {
        // Clears readme_running even if the task panics.
        let _guard = ReadmeRunningGuard(app.clone());
        match run_readme_queue_task(app.clone()).await {
            Ok(_) => {}
            Err(e) => {
                emit_progress(
                    &app,
                    progress(
                        "readme",
                        0,
                        0,
                        format!("README queue paused: {}", e.message),
                        Some(e.message.clone()),
                    ),
                );
            }
        }
    });
}

struct ReadmeRunningGuard(AppHandle);

impl Drop for ReadmeRunningGuard {
    fn drop(&mut self) {
        self.0
            .state::<SyncState>()
            .readme_running
            .store(false, Ordering::SeqCst);
    }
}

async fn run_readme_queue_task(app: AppHandle) -> AppResult<i64> {
    let pat = crate::services::secrets::get_pat()?
        .ok_or_else(|| AppError::auth("GitHub PAT is not configured"))?;
    let client = GitHubClient::production(pat)?;
    fetch_readme_queue(&app, &client).await
}

async fn full_sync(app: &AppHandle, client: &GitHubClient) -> AppResult<DiffStats> {
    let mut all = Vec::new();
    let mut next: Option<String> = None;
    let mut page_etag: Option<String> = None;
    let mut page_num = 0u32;

    loop {
        page_num += 1;
        let page = client.fetch_starred_page(next.as_deref(), None).await?;
        if page_num == 1 {
            page_etag = page.etag.clone();
        }
        emit_progress(
            app,
            progress(
                "full",
                page_num,
                0,
                format!("Fetched page {page_num} ({} repos)", page.repos.len()),
                None,
            ),
        );
        all.extend(page.repos);
        match page.next_url {
            Some(url) => next = Some(url),
            None => break,
        }
    }

    if let Some(etag) = page_etag {
        with_db(app, |conn| settings::set_value(conn, KEY_STARRED_ETAG, &etag))?;
    }

    with_db(app, |conn| apply_full_diff(conn, &all))
}

async fn incremental_sync(app: &AppHandle, client: &GitHubClient) -> AppResult<DiffStats> {
    let etag = with_db(app, |conn| settings::get_value(conn, KEY_STARRED_ETAG))?;
    let first = client.fetch_starred_page(None, etag.as_deref()).await?;

    if first.not_modified {
        emit_progress(
            app,
            progress("incremental", 1, 1, "No changes (ETag 304)", None),
        );
        return Ok(DiffStats::default());
    }

    if let Some(etag) = &first.etag {
        with_db(app, |conn| settings::set_value(conn, KEY_STARRED_ETAG, etag))?;
    }

    let known = with_db(app, load_known_star_pairs)?;
    let mut collected = Vec::new();
    let mut page_num = 0u32;
    let mut current_page = first.repos;
    let mut current_next = first.next_url;

    loop {
        page_num += 1;
        emit_progress(
            app,
            progress(
                "incremental",
                page_num,
                0,
                format!("Fetched page {page_num}"),
                None,
            ),
        );

        let all_known = page_all_known(&current_page, &known);
        collected.extend(current_page);

        if all_known {
            break;
        }

        let Some(url) = current_next else {
            break;
        };
        let page = client.fetch_starred_page(Some(&url), None).await?;
        current_page = page.repos;
        current_next = page.next_url;
    }

    with_db(app, |conn| apply_upserts_only(conn, &collected))
}

fn page_all_known(repos: &[StarredRepo], known: &HashMap<i64, String>) -> bool {
    !repos.is_empty()
        && repos.iter().all(|r| {
            known
                .get(&r.id)
                .map(|s| s == &r.starred_at)
                .unwrap_or(false)
        })
}

fn load_known_star_pairs(conn: &Connection) -> AppResult<HashMap<i64, String>> {
    let mut stmt = conn.prepare("SELECT id, starred_at FROM repos WHERE unstarred = 0")?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let mut map = HashMap::new();
    for row in rows {
        let (id, starred_at) = row?;
        map.insert(id, starred_at);
    }
    Ok(map)
}

pub fn apply_full_diff(conn: &Connection, remote: &[StarredRepo]) -> AppResult<DiffStats> {
    let remote_ids: HashSet<i64> = remote.iter().map(|r| r.id).collect();
    let local_active = load_active_ids(conn)?;
    let to_unstar = diff_unstarred(&local_active, &remote_ids);

    let mut stats = DiffStats::default();
    for repo in remote {
        match upsert_repo(conn, repo)? {
            UpsertKind::Added => stats.added += 1,
            UpsertKind::Updated => stats.updated += 1,
        }
    }
    for id in to_unstar {
        conn.execute("UPDATE repos SET unstarred = 1 WHERE id = ?1", [id])?;
        stats.removed += 1;
    }
    Ok(stats)
}

fn apply_upserts_only(conn: &Connection, remote: &[StarredRepo]) -> AppResult<DiffStats> {
    let mut stats = DiffStats::default();
    for repo in remote {
        match upsert_repo(conn, repo)? {
            UpsertKind::Added => stats.added += 1,
            UpsertKind::Updated => stats.updated += 1,
        }
    }
    Ok(stats)
}

fn load_active_ids(conn: &Connection) -> AppResult<HashSet<i64>> {
    let mut stmt = conn.prepare("SELECT id FROM repos WHERE unstarred = 0")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    let mut set = HashSet::new();
    for row in rows {
        set.insert(row?);
    }
    Ok(set)
}

enum UpsertKind {
    Added,
    Updated,
}

fn upsert_repo(conn: &Connection, repo: &StarredRepo) -> AppResult<UpsertKind> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM repos WHERE id = ?1)",
        [repo.id],
        |row| row.get(0),
    )?;

    let fetched_at = now_rfc3339();
    conn.execute(
        "INSERT INTO repos (
            id, full_name, owner, name, description, language, topics,
            stars_count, forks_count, open_issues, license, homepage, html_url,
            archived, fork, repo_created_at, pushed_at, starred_at, fetched_at, unstarred
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19, 0
         )
         ON CONFLICT(id) DO UPDATE SET
            full_name = excluded.full_name,
            owner = excluded.owner,
            name = excluded.name,
            description = excluded.description,
            language = excluded.language,
            topics = excluded.topics,
            stars_count = excluded.stars_count,
            forks_count = excluded.forks_count,
            open_issues = excluded.open_issues,
            license = excluded.license,
            homepage = excluded.homepage,
            html_url = excluded.html_url,
            archived = excluded.archived,
            fork = excluded.fork,
            repo_created_at = excluded.repo_created_at,
            pushed_at = excluded.pushed_at,
            starred_at = excluded.starred_at,
            fetched_at = excluded.fetched_at,
            unstarred = 0",
        params![
            repo.id,
            repo.full_name,
            repo.owner,
            repo.name,
            repo.description,
            repo.language,
            repo.topics,
            repo.stars_count,
            repo.forks_count,
            repo.open_issues,
            repo.license,
            repo.homepage,
            repo.html_url,
            i64::from(repo.archived),
            i64::from(repo.fork),
            repo.repo_created_at,
            repo.pushed_at,
            repo.starred_at,
            fetched_at,
        ],
    )?;

    Ok(if exists {
        UpsertKind::Updated
    } else {
        UpsertKind::Added
    })
}

fn count_pending_readmes(conn: &Connection) -> AppResult<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM repos WHERE readme_excerpt IS NULL AND unstarred = 0",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}

fn load_pending_readmes(conn: &Connection) -> AppResult<Vec<(i64, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, name FROM repos
         WHERE readme_excerpt IS NULL AND unstarred = 0
         ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Shared spacing gate so concurrent workers still respect a global RPS budget.
pub async fn acquire_rate_slot(limiter: &Mutex<Instant>, min_interval: Duration) {
    let mut last = limiter.lock().await;
    let now = Instant::now();
    let elapsed = now.saturating_duration_since(*last);
    if elapsed < min_interval {
        tokio::time::sleep(min_interval - elapsed).await;
    }
    *last = Instant::now();
}

async fn fetch_readme_queue(app: &AppHandle, client: &GitHubClient) -> AppResult<i64> {
    let started = now_rfc3339();
    let log_id = with_db(app, |conn| begin_sync_log(conn, "readme", &started))?;
    let mut total_fetched = 0i64;

    loop {
        let pending = with_db(app, load_pending_readmes)?;
        if pending.is_empty() {
            break;
        }

        let batch_total = pending.len() as u32;
        emit_progress(
            app,
            progress(
                "readme",
                0,
                batch_total,
                format!("Fetching {batch_total} README excerpts…"),
                None,
            ),
        );

        match fetch_readme_batch(app, client, pending).await {
            Ok(n) => total_fetched += n,
            Err(e) => {
                let finished = now_rfc3339();
                let msg = e.message.clone();
                let _ = with_db(app, |conn| {
                    finish_sync_log(
                        conn,
                        log_id,
                        &finished,
                        &DiffStats {
                            added: 0,
                            updated: total_fetched,
                            removed: 0,
                        },
                        "error",
                        Some(&msg),
                    )
                });
                return Err(e);
            }
        }
    }

    let finished = now_rfc3339();
    with_db(app, |conn| {
        finish_sync_log(
            conn,
            log_id,
            &finished,
            &DiffStats {
                added: 0,
                updated: total_fetched,
                removed: 0,
            },
            "ok",
            None,
        )
    })?;

    emit_progress(
        app,
        progress(
            "readme",
            total_fetched.max(0) as u32,
            total_fetched.max(0) as u32,
            format!("README queue complete ({total_fetched})"),
            None,
        ),
    );

    Ok(total_fetched)
}

fn is_rate_limit_error(err: &AppError) -> bool {
    let msg = err.message.to_ascii_lowercase();
    msg.contains("http 403")
        || msg.contains("http 429")
        || msg.contains("rate limit")
        || msg.contains("secondary rate")
}

async fn fetch_readme_batch(
    app: &AppHandle,
    client: &GitHubClient,
    pending: Vec<(i64, String, String)>,
) -> AppResult<i64> {
    let total = pending.len() as u32;
    let completed = AtomicU32::new(0);
    let fetched = AtomicI64::new(0);
    let skipped = AtomicI64::new(0);
    let stop = AtomicBool::new(false);
    let pause_error: Mutex<Option<AppError>> = Mutex::new(None);
    let limiter = Mutex::new(
        Instant::now()
            .checked_sub(README_MIN_INTERVAL)
            .unwrap_or_else(Instant::now),
    );

    stream::iter(pending)
        .for_each_concurrent(README_CONCURRENCY, |(id, owner, name)| {
            let client = client.clone();
            let app = app.clone();
            let completed = &completed;
            let fetched = &fetched;
            let skipped = &skipped;
            let stop = &stop;
            let pause_error = &pause_error;
            let limiter = &limiter;

            async move {
                if stop.load(Ordering::SeqCst) {
                    return;
                }

                acquire_rate_slot(limiter, README_MIN_INTERVAL).await;

                if stop.load(Ordering::SeqCst) {
                    return;
                }

                // Soft-fail most errors: mark empty so the queue advances.
                // Hard-pause only on rate limits so Resume can retry remaining NULLs.
                let (excerpt, soft_failed) = match client.fetch_readme(&owner, &name).await {
                    Ok(Some(text)) => (text, false),
                    Ok(None) => (String::new(), false),
                    Err(e) if is_rate_limit_error(&e) => {
                        stop.store(true, Ordering::SeqCst);
                        let mut slot = pause_error.lock().await;
                        if slot.is_none() {
                            *slot = Some(e);
                        }
                        return;
                    }
                    Err(_) => (String::new(), true),
                };

                if let Err(e) = with_db(&app, |conn| {
                    conn.execute(
                        "UPDATE repos SET readme_excerpt = ?1 WHERE id = ?2",
                        params![excerpt, id],
                    )?;
                    Ok(())
                }) {
                    // DB errors are local — pause so we don't lose the queue.
                    stop.store(true, Ordering::SeqCst);
                    let mut slot = pause_error.lock().await;
                    if slot.is_none() {
                        *slot = Some(e);
                    }
                    return;
                }

                fetched.fetch_add(1, Ordering::SeqCst);
                if soft_failed {
                    skipped.fetch_add(1, Ordering::SeqCst);
                }
                let current = completed.fetch_add(1, Ordering::SeqCst) + 1;
                let note = if soft_failed {
                    format!("README {current}/{total}: {owner}/{name} (unavailable, skipped)")
                } else {
                    format!("README {current}/{total}: {owner}/{name}")
                };
                emit_progress(&app, progress("readme", current, total, note, None));
            }
        })
        .await;

    if let Some(err) = pause_error.lock().await.take() {
        return Err(err);
    }

    Ok(fetched.load(Ordering::SeqCst))
}

fn begin_sync_log(conn: &Connection, kind: &str, started_at: &str) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO sync_log (started_at, kind, status) VALUES (?1, ?2, 'running')",
        params![started_at, kind],
    )?;
    Ok(conn.last_insert_rowid())
}

fn finish_sync_log(
    conn: &Connection,
    id: i64,
    finished_at: &str,
    stats: &DiffStats,
    status: &str,
    error: Option<&str>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE sync_log SET
            finished_at = ?1,
            repos_added = ?2,
            repos_updated = ?3,
            repos_removed = ?4,
            status = ?5,
            error = ?6
         WHERE id = ?7",
        params![
            finished_at,
            stats.added,
            stats.updated,
            stats.removed,
            status,
            error,
            id
        ],
    )?;
    Ok(())
}

fn progress(
    kind: impl Into<String>,
    current: u32,
    total: u32,
    message: impl Into<String>,
    error: Option<String>,
) -> SyncProgress {
    SyncProgress {
        kind: kind.into(),
        current,
        total,
        message: message.into(),
        error,
    }
}

fn emit_progress(app: &AppHandle, progress: SyncProgress) {
    let _ = app.emit("sync://progress", progress);
}

fn with_db<T, F>(app: &AppHandle, f: F) -> AppResult<T>
where
    F: FnOnce(&Connection) -> AppResult<T>,
{
    let state = app.state::<DbState>();
    let conn = state
        .0
        .lock()
        .map_err(|_| AppError::db("database lock poisoned"))?;
    f(&conn)
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
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
        let path = std::env::temp_dir().join(format!("starboard_sync_{nanos}.db"));
        open_and_migrate(&path).expect("migrate")
    }

    fn sample_repo(id: i64, name: &str, starred_at: &str) -> StarredRepo {
        StarredRepo {
            id,
            full_name: format!("owner/{name}"),
            owner: "owner".into(),
            name: name.into(),
            description: Some(format!("{name} desc")),
            language: Some("Rust".into()),
            topics: "[\"cli\"]".into(),
            stars_count: Some(10),
            forks_count: Some(1),
            open_issues: Some(0),
            license: Some("MIT".into()),
            homepage: None,
            html_url: format!("https://github.com/owner/{name}"),
            archived: false,
            fork: false,
            repo_created_at: Some("2020-01-01T00:00:00Z".into()),
            pushed_at: Some("2024-01-01T00:00:00Z".into()),
            starred_at: starred_at.into(),
        }
    }

    #[test]
    fn unstar_soft_delete_diff() {
        let local: HashSet<i64> = [1, 2, 3].into_iter().collect();
        let remote: HashSet<i64> = [1, 3].into_iter().collect();
        let mut missing = diff_unstarred(&local, &remote);
        missing.sort_unstable();
        assert_eq!(missing, vec![2]);
    }

    #[test]
    fn full_diff_marks_unstarred_and_upserts() {
        let conn = test_conn();
        let first = vec![
            sample_repo(1, "a", "2024-01-01T00:00:00Z"),
            sample_repo(2, "b", "2024-01-02T00:00:00Z"),
        ];
        let stats = apply_full_diff(&conn, &first).expect("diff");
        assert_eq!(stats.added, 2);

        let second = vec![sample_repo(1, "a", "2024-01-01T00:00:00Z")];
        let stats = apply_full_diff(&conn, &second).expect("diff2");
        assert_eq!(stats.removed, 1);
        assert_eq!(stats.updated, 1);

        let unstarred: i64 = conn
            .query_row("SELECT unstarred FROM repos WHERE id = 2", [], |r| r.get(0))
            .expect("row");
        assert_eq!(unstarred, 1);
    }

    #[test]
    fn fts_trigger_indexes_inserted_repo() {
        let conn = test_conn();
        let repo = sample_repo(42, "searchme", "2024-06-01T00:00:00Z");
        apply_full_diff(&conn, &[repo]).expect("upsert");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM repos_fts WHERE repos_fts MATCH 'searchme'",
                [],
                |r| r.get(0),
            )
            .expect("fts");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn rate_slot_enforces_minimum_spacing() {
        let limiter = Mutex::new(
            Instant::now()
                .checked_sub(Duration::from_millis(50))
                .unwrap_or_else(Instant::now),
        );
        let interval = Duration::from_millis(40);
        let start = Instant::now();
        acquire_rate_slot(&limiter, interval).await;
        acquire_rate_slot(&limiter, interval).await;
        acquire_rate_slot(&limiter, interval).await;
        assert!(start.elapsed() >= Duration::from_millis(80));
    }
}
