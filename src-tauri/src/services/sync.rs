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
use crate::services::github::{GitHubClient, ReadmeFetch};
use crate::services::jobs::CancelFlag;
use crate::services::settings::{
    self, KEY_INCREMENTAL_SINCE_RECONCILE, KEY_LAST_FULL_RECONCILE_AT, KEY_LAST_SYNCED_AT,
    KEY_STARRED_ETAG,
};
use crate::services::store::{self, DbState};

/// Incremental `/user/starred` pagination stops at the first page of already-known
/// `(id, starred_at)` pairs and never walks the tail. It therefore **cannot**
/// detect unstars. Claiming otherwise would be a production lie.
///
/// Policy:
/// - ETag 304 still means "the starred list is unchanged" (including no unstars).
/// - Incremental Sync upserts new/changed stars only (`repos_removed` stays 0).
/// - Unstars are applied only on a full library walk (Full sync).
/// - Incremental Sync auto-upgrades to a full reconcile after 7 days **or**
///   7 incremental Syncs, whichever comes first.
///
/// Users can always run Full sync immediately.
pub const UNSTAR_POLICY: &str = "Incremental Sync updates new and changed stars only. \
Unstars are detected on Full sync (automatic every 7 days or 7 incremental Syncs). \
A 304 ETag response still means the starred list is unchanged.";

const RECONCILE_EVERY_N_INCREMENTAL: i64 = 7;
const RECONCILE_AFTER_SECS: i64 = 7 * 24 * 60 * 60;

/// Max in-flight README requests.
const README_CONCURRENCY: usize = 6;
/// Minimum spacing between README request starts (~6 req/s).
const README_MIN_INTERVAL: Duration = Duration::from_millis(167);

pub struct SyncState {
    pub running: AtomicBool,
    pub readme_running: AtomicBool,
    pub cancel_sync: CancelFlag,
    pub cancel_readme: CancelFlag,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            running: AtomicBool::new(false),
            readme_running: AtomicBool::new(false),
            cancel_sync: CancelFlag::default(),
            cancel_readme: CancelFlag::default(),
        }
    }
}

pub fn request_cancel_sync(state: &SyncState) -> AppResult<()> {
    if !state.running.load(Ordering::SeqCst) {
        return Err(AppError::sync("no sync is running"));
    }
    state.cancel_sync.request();
    Ok(())
}

pub fn request_cancel_readme(state: &SyncState) -> AppResult<()> {
    if !state.readme_running.load(Ordering::SeqCst) {
        return Err(AppError::sync("README queue is not running"));
    }
    state.cancel_readme.request();
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct DiffStats {
    pub added: i64,
    pub updated: i64,
    pub removed: i64,
}

/// Pure sync-diff helper: soft-deletes local ids missing from remote.
pub fn diff_unstarred(local_active_ids: &HashSet<i64>, remote_ids: &HashSet<i64>) -> Vec<i64> {
    local_active_ids.difference(remote_ids).copied().collect()
}

pub fn get_sync_status(
    conn: &Connection,
    running: bool,
    readme_running: bool,
) -> AppResult<SyncStatus> {
    let last_synced_at = settings::get_value(conn, KEY_LAST_SYNCED_AT)?;
    let last_result = latest_sync_result(conn)?;
    let pending_readmes = count_pending_readmes(conn)?;
    let last_full_reconcile_at = settings::get_value(conn, KEY_LAST_FULL_RECONCILE_AT)?;
    let due = reconcile_due(conn)?;
    Ok(SyncStatus {
        running,
        readme_running,
        pending_readmes,
        last_synced_at,
        last_result,
        last_full_reconcile_at,
        reconcile_due: due,
        unstar_policy: UNSTAR_POLICY.to_string(),
    })
}

/// See [`UNSTAR_POLICY`]: incremental cannot detect unstars.
pub fn reconcile_due(conn: &Connection) -> AppResult<bool> {
    let count = settings::get_value(conn, KEY_INCREMENTAL_SINCE_RECONCILE)?
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    if count >= RECONCILE_EVERY_N_INCREMENTAL {
        return Ok(true);
    }
    match settings::get_value(conn, KEY_LAST_FULL_RECONCILE_AT)? {
        None => {
            let has_repos: i64 =
                conn.query_row("SELECT COUNT(*) FROM repos", [], |row| row.get(0))?;
            Ok(has_repos > 0)
        }
        Some(iso) => {
            let Ok(parsed) =
                OffsetDateTime::parse(&iso, &time::format_description::well_known::Rfc3339)
            else {
                return Ok(true);
            };
            let age = OffsetDateTime::now_utc() - parsed;
            Ok(age.whole_seconds() >= RECONCILE_AFTER_SECS)
        }
    }
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
    sync_state.cancel_sync.reset();

    let result = run_sync_inner(&app, full).await;
    sync_state.running.store(false, Ordering::SeqCst);
    result
}

async fn run_sync_inner(app: &AppHandle, full: bool) -> AppResult<SyncResult> {
    let pat = crate::services::secrets::get_pat()?
        .ok_or_else(|| AppError::auth("GitHub PAT is not configured"))?;
    let client = GitHubClient::production(pat)?;

    let promote_to_full = if full {
        true
    } else {
        with_db(app, reconcile_due)?
    };
    let kind = if promote_to_full {
        "full"
    } else {
        "incremental"
    };
    let started_at = now_rfc3339();
    let log_id = with_db(app, |conn| begin_sync_log(conn, kind, &started_at))?;

    let start_message = if !full && promote_to_full {
        "Periodic full reconcile (unstar detection)…"
    } else {
        "Fetching starred repositories…"
    };
    emit_progress(app, progress(kind, 0, 0, start_message, None));

    let sync_outcome = if promote_to_full {
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
            let status = if e.is_cancelled() {
                "cancelled"
            } else {
                "error"
            };
            let msg = e.message.clone();
            let _ = with_db(app, |conn| {
                finish_sync_log(conn, log_id, &finished, &empty, status, Some(&msg))
            });
            emit_progress(
                app,
                progress(
                    kind,
                    0,
                    0,
                    format!("Sync {status}: {msg}"),
                    Some(msg.clone()),
                ),
            );
            Err(e)
        }
    }
}

/// Start the README queue if there is pending work and no queue is already running.
///
/// README-then-embed ordering: repo embed documents include `readme_excerpt`, which this
/// queue fills after sync. Embedding first would immediately stale those rows. So we:
/// - if no READMEs pending → kick `embed::spawn_embed_pipeline_if_needed` now;
/// - if READMEs pending → run the queue, then kick auto-embed when it drains successfully.
///
/// Launch (`lib.rs` setup) and post-sync both enter through this function, so one hook covers
/// both. Auto-embed is a silent no-op when Ollama is offline or nothing is stale.
pub fn spawn_readme_queue_if_needed(app: AppHandle) {
    let pending = match with_db(&app, count_pending_readmes) {
        Ok(n) => n,
        Err(_) => return,
    };
    if pending == 0 {
        crate::services::embed::spawn_embed_pipeline_if_needed(app);
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
    sync_state.cancel_readme.reset();

    tauri::async_runtime::spawn(async move {
        // Clears readme_running even if the task panics.
        let _guard = ReadmeRunningGuard(app.clone());
        match run_readme_queue_task(app.clone()).await {
            Ok(_) => {
                crate::services::embed::spawn_embed_pipeline_if_needed(app.clone());
            }
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
        check_sync_cancel(app)?;
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

    // Persist ETag only after the full apply commits. A failed apply must
    // never leave a newer ETag (would hide the failed page of work on 304).
    with_db(app, |conn| {
        commit_full_apply(conn, &all, page_etag.as_deref())
    })
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

    let known = with_db(app, load_known_star_pairs)?;
    let mut collected = Vec::new();
    let mut page_num = 0u32;
    let mut current_page = first.repos;
    let mut current_next = first.next_url;

    loop {
        check_sync_cancel(app)?;
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

    // Incremental never marks unstars — see UNSTAR_POLICY. ETag is written
    // in the same transaction as the upserts.
    with_db(app, |conn| {
        commit_incremental_apply(conn, &collected, first.etag.as_deref())
    })
}

fn check_sync_cancel(app: &AppHandle) -> AppResult<()> {
    app.state::<SyncState>().cancel_sync.check()
}

/// Apply a full remote snapshot + persist ETag + record reconcile time.
/// All-or-nothing: apply failure rolls back the ETag write.
pub fn commit_full_apply(
    conn: &Connection,
    remote: &[StarredRepo],
    etag: Option<&str>,
) -> AppResult<DiffStats> {
    store::with_tx(conn, |tx| {
        let stats = apply_full_diff(tx, remote)?;
        if let Some(tag) = etag {
            settings::set_value(tx, KEY_STARRED_ETAG, tag)?;
        }
        settings::set_value(tx, KEY_LAST_FULL_RECONCILE_AT, &now_rfc3339())?;
        settings::set_value(tx, KEY_INCREMENTAL_SINCE_RECONCILE, "0")?;
        Ok(stats)
    })
}

/// Upsert-only apply + ETag. Does not mark unstars.
pub fn commit_incremental_apply(
    conn: &Connection,
    remote: &[StarredRepo],
    etag: Option<&str>,
) -> AppResult<DiffStats> {
    store::with_tx(conn, |tx| {
        let stats = apply_upserts_only(tx, remote)?;
        if let Some(tag) = etag {
            settings::set_value(tx, KEY_STARRED_ETAG, tag)?;
        }
        let prev = settings::get_value(tx, KEY_INCREMENTAL_SINCE_RECONCILE)?
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        settings::set_value(
            tx,
            KEY_INCREMENTAL_SINCE_RECONCILE,
            &(prev.saturating_add(1)).to_string(),
        )?;
        Ok(stats)
    })
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

    let existing_readme: Option<String> = if exists {
        conn.query_row(
            "SELECT readme_excerpt FROM repos WHERE id = ?1",
            [repo.id],
            |row| row.get(0),
        )
        .ok()
        .flatten()
    } else {
        None
    };
    let document_hash =
        crate::services::embed::content_hash(&crate::services::embed::build_document(
            &repo.full_name,
            repo.description.as_deref(),
            &repo.topics,
            existing_readme.as_deref(),
        ));

    let fetched_at = now_rfc3339();
    conn.execute(
        "INSERT INTO repos (
            id, full_name, owner, name, description, language, topics,
            stars_count, forks_count, open_issues, license, homepage, html_url,
            archived, fork, repo_created_at, pushed_at, starred_at, fetched_at, unstarred,
            document_hash
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19, 0,
            ?20
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
            unstarred = 0,
            document_hash = excluded.document_hash",
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
            document_hash,
        ],
    )?;

    Ok(if exists {
        UpsertKind::Updated
    } else {
        UpsertKind::Added
    })
}

fn pending_readme_sql() -> &'static str {
    "unstarred = 0 AND (readme_excerpt IS NULL OR readme_status = 'retryable')"
}

fn count_pending_readmes(conn: &Connection) -> AppResult<i64> {
    let sql = format!("SELECT COUNT(*) FROM repos WHERE {}", pending_readme_sql());
    let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(count)
}

fn load_pending_readmes(conn: &Connection) -> AppResult<Vec<(i64, String, String)>> {
    let sql = format!(
        "SELECT id, owner, name FROM repos
         WHERE {}
         ORDER BY id",
        pending_readme_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
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
                let status = if e.is_cancelled() {
                    "cancelled"
                } else {
                    "error"
                };
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
                        status,
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
            let stop = &stop;
            let pause_error = &pause_error;
            let limiter = &limiter;

            async move {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                if app.state::<SyncState>().cancel_readme.is_cancelled() {
                    stop.store(true, Ordering::SeqCst);
                    let mut slot = pause_error.lock().await;
                    if slot.is_none() {
                        *slot = Some(AppError::cancelled("README queue cancelled"));
                    }
                    return;
                }

                acquire_rate_slot(limiter, README_MIN_INTERVAL).await;

                if stop.load(Ordering::SeqCst) {
                    return;
                }
                if app.state::<SyncState>().cancel_readme.is_cancelled() {
                    stop.store(true, Ordering::SeqCst);
                    let mut slot = pause_error.lock().await;
                    if slot.is_none() {
                        *slot = Some(AppError::cancelled("README queue cancelled"));
                    }
                    return;
                }

                // Confirmed 404 → empty excerpt + status=missing (do not retry).
                // Transient/5xx → leave excerpt NULL, status=retryable (Resume picks up).
                // Hard-pause only on rate limits so Resume can retry remaining NULLs.
                let write = match client.fetch_readme(&owner, &name).await {
                    Ok(ReadmeFetch::Excerpt(text)) => Some((text, "ok", None::<String>)),
                    Ok(ReadmeFetch::NotFound) => Some((String::new(), "missing", None)),
                    Err(e) if is_rate_limit_error(&e) => {
                        stop.store(true, Ordering::SeqCst);
                        let mut slot = pause_error.lock().await;
                        if slot.is_none() {
                            *slot = Some(e);
                        }
                        return;
                    }
                    Err(e) => {
                        if let Err(db_err) =
                            with_db(&app, |conn| mark_readme_retryable(conn, id, &e.message))
                        {
                            stop.store(true, Ordering::SeqCst);
                            let mut slot = pause_error.lock().await;
                            if slot.is_none() {
                                *slot = Some(db_err);
                            }
                        } else {
                            let current = completed.fetch_add(1, Ordering::SeqCst) + 1;
                            emit_progress(
                                &app,
                                progress(
                                    "readme",
                                    current,
                                    total,
                                    format!(
                                        "README {current}/{total}: {owner}/{name} (retry later)"
                                    ),
                                    None,
                                ),
                            );
                        }
                        return;
                    }
                };

                let Some((excerpt, status, _err)) = write else {
                    return;
                };

                if let Err(e) = with_db(&app, |conn| {
                    store_readme_excerpt(conn, id, &excerpt, status)
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
                let current = completed.fetch_add(1, Ordering::SeqCst) + 1;
                let note = if status == "missing" {
                    format!("README {current}/{total}: {owner}/{name} (no README)")
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

fn store_readme_excerpt(
    conn: &Connection,
    repo_id: i64,
    excerpt: &str,
    status: &str,
) -> AppResult<()> {
    let (full_name, description, topics): (String, Option<String>, String) = conn.query_row(
        "SELECT full_name, description, topics FROM repos WHERE id = ?1",
        [repo_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let hash = crate::services::embed::content_hash(&crate::services::embed::build_document(
        &full_name,
        description.as_deref(),
        &topics,
        Some(excerpt),
    ));
    conn.execute(
        "UPDATE repos SET
            readme_excerpt = ?1,
            readme_status = ?2,
            readme_last_error = NULL,
            document_hash = ?3
         WHERE id = ?4",
        params![excerpt, status, hash, repo_id],
    )?;
    Ok(())
}

fn mark_readme_retryable(conn: &Connection, repo_id: i64, error: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE repos SET
            readme_status = 'retryable',
            readme_attempts = COALESCE(readme_attempts, 0) + 1,
            readme_last_error = ?1
         WHERE id = ?2",
        params![error, repo_id],
    )?;
    Ok(())
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

    #[test]
    fn etag_is_not_advanced_when_transactional_apply_fails() {
        let conn = test_conn();
        settings::set_value(&conn, KEY_STARRED_ETAG, "old-etag").expect("seed etag");
        let err = store::with_tx(&conn, |tx| {
            apply_full_diff(tx, &[sample_repo(1, "a", "2024-01-01T00:00:00Z")])?;
            settings::set_value(tx, KEY_STARRED_ETAG, "new-etag")?;
            Err::<(), _>(AppError::db("injected apply failure"))
        })
        .expect_err("fail");
        assert_eq!(err.message, "injected apply failure");
        let etag = settings::get_value(&conn, KEY_STARRED_ETAG)
            .expect("get")
            .expect("etag");
        assert_eq!(etag, "old-etag");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM repos", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 0);
    }

    #[test]
    fn commit_full_apply_persists_etag_and_reconcile_time() {
        let conn = test_conn();
        let stats = commit_full_apply(
            &conn,
            &[sample_repo(1, "a", "2024-01-01T00:00:00Z")],
            Some("\"etag-ok\""),
        )
        .expect("commit");
        assert_eq!(stats.added, 1);
        assert_eq!(
            settings::get_value(&conn, KEY_STARRED_ETAG)
                .expect("etag")
                .as_deref(),
            Some("\"etag-ok\"")
        );
        assert!(settings::get_value(&conn, KEY_LAST_FULL_RECONCILE_AT)
            .expect("reconcile")
            .is_some());
        assert_eq!(
            settings::get_value(&conn, KEY_INCREMENTAL_SINCE_RECONCILE)
                .expect("count")
                .as_deref(),
            Some("0")
        );
    }

    #[test]
    fn incremental_apply_never_marks_unstars() {
        let conn = test_conn();
        apply_full_diff(
            &conn,
            &[
                sample_repo(1, "a", "2024-01-01T00:00:00Z"),
                sample_repo(2, "b", "2024-01-02T00:00:00Z"),
            ],
        )
        .expect("seed");
        let stats = commit_incremental_apply(
            &conn,
            &[sample_repo(1, "a", "2024-01-01T00:00:00Z")],
            Some("\"inc\""),
        )
        .expect("inc");
        assert_eq!(stats.removed, 0);
        let unstarred: i64 = conn
            .query_row("SELECT unstarred FROM repos WHERE id = 2", [], |r| r.get(0))
            .expect("row");
        assert_eq!(unstarred, 0, "incremental must not claim unstar detection");
        assert_eq!(
            settings::get_value(&conn, KEY_INCREMENTAL_SINCE_RECONCILE)
                .expect("n")
                .as_deref(),
            Some("1")
        );
    }

    #[test]
    fn reconcile_due_after_seven_incrementals() {
        let conn = test_conn();
        settings::set_value(&conn, KEY_LAST_FULL_RECONCILE_AT, &now_rfc3339()).expect("rec");
        settings::set_value(&conn, KEY_INCREMENTAL_SINCE_RECONCILE, "7").expect("n");
        assert!(reconcile_due(&conn).expect("due"));
        settings::set_value(&conn, KEY_INCREMENTAL_SINCE_RECONCILE, "1").expect("n");
        assert!(!reconcile_due(&conn).expect("not due"));
    }

    #[test]
    fn readme_pending_includes_retryable_not_missing() {
        let conn = test_conn();
        apply_full_diff(&conn, &[sample_repo(1, "a", "2024-01-01T00:00:00Z")]).expect("seed");
        assert_eq!(count_pending_readmes(&conn).expect("pending"), 1);

        store_readme_excerpt(&conn, 1, "", "missing").expect("404");
        assert_eq!(count_pending_readmes(&conn).expect("missing"), 0);

        conn.execute(
            "UPDATE repos SET readme_excerpt = NULL, readme_status = NULL WHERE id = 1",
            [],
        )
        .expect("reset");
        mark_readme_retryable(&conn, 1, "HTTP 503: unavailable").expect("retry");
        assert_eq!(count_pending_readmes(&conn).expect("retryable"), 1);
        let status: String = conn
            .query_row("SELECT readme_status FROM repos WHERE id = 1", [], |r| {
                r.get(0)
            })
            .expect("status");
        assert_eq!(status, "retryable");
    }

    #[test]
    fn cancel_flag_finishes_sync_log_as_cancelled() {
        let conn = test_conn();
        let id = begin_sync_log(&conn, "full", "2024-01-01T00:00:00Z").expect("begin");
        finish_sync_log(
            &conn,
            id,
            "2024-01-01T00:00:01Z",
            &DiffStats::default(),
            "cancelled",
            Some("operation cancelled"),
        )
        .expect("finish");
        let status: String = conn
            .query_row("SELECT status FROM sync_log WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .expect("status");
        assert_eq!(status, "cancelled");
    }
}
