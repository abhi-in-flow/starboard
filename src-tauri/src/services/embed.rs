use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use time::OffsetDateTime;

use crate::error::{AppError, AppResult};
use crate::models::{EmbedProgress, EmbedStatus};
use crate::services::jobs::{CancelFlag, RunningFlagGuard};
use crate::services::ollama::OllamaClient;
use crate::services::settings;
use crate::services::store::{self, DbState};

/// Repos per `/api/embed` call (task: batch e.g. 32).
const EMBED_BATCH_SIZE: usize = 32;

pub struct EmbedState {
    pub running: AtomicBool,
    pub last_error: Mutex<Option<String>>,
    pub cancel: CancelFlag,
}

impl Default for EmbedState {
    fn default() -> Self {
        Self {
            running: AtomicBool::new(false),
            last_error: Mutex::new(None),
            cancel: CancelFlag::default(),
        }
    }
}

pub fn request_cancel(state: &EmbedState) -> AppResult<()> {
    if !state.running.load(Ordering::SeqCst) {
        return Err(AppError::ollama("embedding pipeline is not running"));
    }
    state.cancel.request();
    Ok(())
}

#[derive(Debug, Clone)]
pub struct EmbedDoc {
    pub repo_id: i64,
    pub document: String,
    pub content_hash: String,
}

/// Build the embedding document string for a repo (HLD §6 / Phase 4 task).
pub fn build_document(
    full_name: &str,
    description: Option<&str>,
    topics: &str,
    readme_excerpt: Option<&str>,
) -> String {
    let topics_display = format_topics(topics);
    format!(
        "{full_name}\n{}\nTopics: {topics_display}\n{}",
        description.unwrap_or(""),
        readme_excerpt.unwrap_or("")
    )
}

#[cfg(test)]
pub fn refresh_document_hash(conn: &Connection, repo_id: i64) -> AppResult<String> {
    let (full_name, description, topics, readme): (String, Option<String>, String, Option<String>) =
        conn.query_row(
            "SELECT full_name, description, topics, readme_excerpt FROM repos WHERE id = ?1",
            [repo_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    let hash = content_hash(&build_document(
        &full_name,
        description.as_deref(),
        &topics,
        readme.as_deref(),
    ));
    conn.execute(
        "UPDATE repos SET document_hash = ?1 WHERE id = ?2",
        params![hash, repo_id],
    )?;
    Ok(hash)
}

pub fn content_hash(document: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(document.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn format_topics(topics_json: &str) -> String {
    match serde_json::from_str::<Vec<String>>(topics_json) {
        Ok(list) if !list.is_empty() => list.join(", "),
        _ => topics_json.trim().to_string(),
    }
}

fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

/// Cheap SQL counts — no per-repo hashing. Relies on persisted `repos.document_hash`.
pub fn embed_coverage_counts(conn: &Connection) -> AppResult<(i64, i64, i64, i64)> {
    let total_repos: i64 = conn.query_row(
        "SELECT COUNT(*) FROM repos WHERE unstarred = 0",
        [],
        |row| row.get(0),
    )?;
    let embedded_repos: i64 = conn.query_row(
        "SELECT COUNT(*) FROM repo_embedding_meta m
         JOIN repos r ON r.id = m.repo_id
         WHERE r.unstarred = 0",
        [],
        |row| row.get(0),
    )?;
    let missing_repos: i64 = conn.query_row(
        "SELECT COUNT(*) FROM repos r
         LEFT JOIN repo_embedding_meta m ON m.repo_id = r.id
         WHERE r.unstarred = 0 AND m.repo_id IS NULL",
        [],
        |row| row.get(0),
    )?;
    let stale_repos: i64 = conn.query_row(
        "SELECT COUNT(*) FROM repos r
         JOIN repo_embedding_meta m ON m.repo_id = r.id
         WHERE r.unstarred = 0
           AND r.document_hash IS NOT NULL
           AND m.content_hash != r.document_hash",
        [],
        |row| row.get(0),
    )?;
    Ok((total_repos, embedded_repos, missing_repos, stale_repos))
}

/// Repos that lack an embedding row or whose stored content_hash is stale.
/// Uses persisted `document_hash` so status/listing does not hash the library.
pub fn list_stale_or_missing(conn: &Connection) -> AppResult<Vec<EmbedDoc>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.full_name, r.description, r.topics, r.readme_excerpt,
                r.document_hash
         FROM repos r
         LEFT JOIN repo_embedding_meta m ON m.repo_id = r.id
         WHERE r.unstarred = 0
           AND (m.repo_id IS NULL
                OR r.document_hash IS NULL
                OR m.content_hash != r.document_hash)
         ORDER BY r.id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (id, full_name, description, topics, readme, stored_doc_hash) = row?;
        let document = build_document(
            &full_name,
            description.as_deref(),
            &topics,
            readme.as_deref(),
        );
        let hash = stored_doc_hash.unwrap_or_else(|| content_hash(&document));
        out.push(EmbedDoc {
            repo_id: id,
            document,
            content_hash: hash,
        });
    }
    Ok(out)
}

pub fn get_embed_status(conn: &Connection, state: &EmbedState) -> AppResult<EmbedStatus> {
    let settings = settings::get_settings(conn)?;
    let (total_repos, embedded_repos, missing, stale) = embed_coverage_counts(conn)?;
    let stale_or_missing = missing + stale;
    let coverage = if total_repos == 0 {
        0.0
    } else {
        embedded_repos as f64 / total_repos as f64
    };
    let last_error = state
        .last_error
        .lock()
        .map_err(|_| AppError::db("embed state lock poisoned"))?
        .clone();
    Ok(EmbedStatus {
        running: state.running.load(Ordering::SeqCst),
        total_repos,
        embedded_repos,
        stale_or_missing,
        coverage,
        last_error,
        model: settings.ollama_embed_model,
        dimension: settings.embed_dimension,
        need_rebuild: settings.embeddings_need_rebuild,
    })
}

pub fn upsert_embedding(
    conn: &Connection,
    repo_id: i64,
    embedding: &[f32],
    content_hash: &str,
    model: &str,
    dimension: i64,
) -> AppResult<()> {
    if embedding.len() as i64 != dimension {
        return Err(AppError::ollama(format!(
            "embedding length {} does not match configured dimension {dimension}",
            embedding.len()
        )));
    }
    let bytes = embedding_to_bytes(embedding);
    let embedded_at = now_rfc3339();
    store::with_tx(conn, |tx| {
        tx.execute(
            "DELETE FROM repo_embeddings WHERE repo_id = ?1",
            params![repo_id],
        )?;
        tx.execute(
            "INSERT INTO repo_embeddings(repo_id, embedding) VALUES (?1, ?2)",
            params![repo_id, bytes],
        )?;
        tx.execute(
            "INSERT INTO repo_embedding_meta (repo_id, content_hash, model, dimension, embedded_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(repo_id) DO UPDATE SET
               content_hash = excluded.content_hash,
               model = excluded.model,
               dimension = excluded.dimension,
               embedded_at = excluded.embedded_at",
            params![repo_id, content_hash, model, dimension, embedded_at],
        )?;
        Ok(())
    })
}

/// Pure trigger gate for launch / post-README auto-embed.
/// Pending = `list_stale_or_missing` length; Ollama reachability from a short health check.
pub fn should_attempt_auto_embed(pending_count: usize, ollama_available: bool) -> bool {
    pending_count > 0 && ollama_available
}

/// Short, non-fatal Ollama probe for auto-embed triggers. Never errors — unreachable ⇒ false.
pub async fn is_ollama_reachable_for_embed(settings: &crate::models::AppSettings) -> bool {
    match OllamaClient::for_embeddings(settings) {
        Ok(client) => client
            .health_check()
            .await
            .map(|s| s.available)
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Background auto-embed: if anything is stale/missing and Ollama is up, run the pipeline.
/// Silent no-op when offline, nothing pending, rebuild required, or already running.
///
/// Prefer calling this after the README queue drains (documents include `readme_excerpt`);
/// `sync::spawn_readme_queue_if_needed` is the post-sync / launch hook that does so.
pub fn spawn_embed_pipeline_if_needed(app: AppHandle) {
    let pending = match with_db(&app, |conn| Ok(list_stale_or_missing(conn)?.len())) {
        Ok(n) => n,
        Err(_) => return,
    };
    if pending == 0 {
        return;
    }
    let need_rebuild = match with_db(&app, settings::get_settings) {
        Ok(s) => s.embeddings_need_rebuild,
        Err(_) => return,
    };
    if need_rebuild {
        return;
    }
    let embed_state = app.state::<EmbedState>();
    if embed_state.running.load(Ordering::SeqCst) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let settings = match with_db(&app, settings::get_settings) {
            Ok(s) => s,
            Err(_) => return,
        };
        let reachable = is_ollama_reachable_for_embed(&settings).await;
        // Re-count after the health probe — README workers may still be writing excerpts.
        let pending = match with_db(&app, |conn| Ok(list_stale_or_missing(conn)?.len())) {
            Ok(n) => n,
            Err(_) => return,
        };
        if !should_attempt_auto_embed(pending, reachable) {
            return;
        }
        let _ = run_embed_pipeline(app).await;
    });
}

/// Testable auto-embed pass (no AppHandle): embeds only stale/missing when Ollama is reachable.
/// Returns the number of repos embedded (0 = silent skip).
#[cfg(test)]
pub async fn auto_embed_stale_if_reachable(conn: &Connection) -> AppResult<usize> {
    let settings = settings::get_settings(conn)?;
    if settings.embeddings_need_rebuild {
        return Ok(0);
    }
    let pending = list_stale_or_missing(conn)?;
    let reachable = is_ollama_reachable_for_embed(&settings).await;
    if !should_attempt_auto_embed(pending.len(), reachable) {
        return Ok(0);
    }
    let client = OllamaClient::for_embeddings(&settings)?;
    let dimension = settings.embed_dimension;
    let model = settings.ollama_embed_model.as_str();
    let mut updated = 0usize;
    for chunk in pending.chunks(EMBED_BATCH_SIZE) {
        let inputs: Vec<String> = chunk.iter().map(|d| d.document.clone()).collect();
        let vectors = client.embed(&inputs).await?;
        for (doc, embedding) in chunk.iter().zip(vectors.iter()) {
            upsert_embedding(
                conn,
                doc.repo_id,
                embedding,
                &doc.content_hash,
                model,
                dimension,
            )?;
            updated += 1;
        }
    }
    Ok(updated)
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

fn progress(
    kind: &str,
    current: u32,
    total: u32,
    message: impl Into<String>,
    error: Option<String>,
) -> EmbedProgress {
    EmbedProgress {
        kind: kind.into(),
        current,
        total,
        message: message.into(),
        error,
    }
}

fn emit_progress(app: &AppHandle, progress: EmbedProgress) {
    let _ = app.emit("embed://progress", progress);
}

fn begin_sync_log(conn: &Connection, started_at: &str) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO sync_log (started_at, kind, status) VALUES (?1, 'embed', 'running')",
        params![started_at],
    )?;
    Ok(conn.last_insert_rowid())
}

fn finish_sync_log(
    conn: &Connection,
    log_id: i64,
    finished_at: &str,
    status: &str,
    error: Option<&str>,
    updated: i64,
) -> AppResult<()> {
    conn.execute(
        "UPDATE sync_log SET
           finished_at = ?1,
           status = ?2,
           error = ?3,
           repos_updated = ?4
         WHERE id = ?5",
        params![finished_at, status, error, updated, log_id],
    )?;
    Ok(())
}

/// Embed all stale/missing repos. Emits `embed://progress`. Resumable via content_hash.
pub async fn run_embed_pipeline(app: AppHandle) -> AppResult<()> {
    let embed_state = app.state::<EmbedState>();
    if embed_state
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::ollama("embedding pipeline is already running"));
    }
    embed_state.cancel.reset();
    if let Ok(mut guard) = embed_state.last_error.lock() {
        *guard = None;
    }
    let _running = RunningFlagGuard::holding(&embed_state.running);

    let result = run_embed_pipeline_inner(&app).await;
    if let Err(ref e) = result {
        if !e.is_cancelled() {
            if let Ok(mut guard) = embed_state.last_error.lock() {
                *guard = Some(e.message.clone());
            }
        }
    }
    result
}

async fn run_embed_pipeline_inner(app: &AppHandle) -> AppResult<()> {
    let app_settings = with_db(app, settings::get_settings)?;
    if app_settings.embeddings_need_rebuild {
        return Err(AppError::settings(
            "embedding dimension changed — rebuild the embeddings table in Settings first",
        ));
    }

    let client = OllamaClient::for_embeddings(&app_settings)?;
    let health = client.health_check().await?;
    if !health.available {
        let msg = format!("Ollama offline — {}", health.message);
        emit_progress(app, progress("embed", 0, 0, msg.clone(), Some(msg.clone())));
        return Err(AppError::ollama(msg));
    }

    let dimension = app_settings.embed_dimension;
    let model = app_settings.ollama_embed_model.clone();
    let started = now_rfc3339();
    let log_id = with_db(app, |conn| begin_sync_log(conn, &started))?;

    let pending = with_db(app, list_stale_or_missing)?;
    let total = pending.len() as u32;
    emit_progress(
        app,
        progress(
            "embed",
            0,
            total,
            if total == 0 {
                "Embeddings up to date"
            } else {
                "Embedding repositories…"
            },
            None,
        ),
    );

    if total == 0 {
        let finished = now_rfc3339();
        with_db(app, |conn| {
            finish_sync_log(conn, log_id, &finished, "ok", None, 0)
        })?;
        return Ok(());
    }

    let mut processed: u32 = 0;
    let mut updated: i64 = 0;

    for chunk in pending.chunks(EMBED_BATCH_SIZE) {
        if let Err(e) = app.state::<EmbedState>().cancel.check() {
            let finished = now_rfc3339();
            let msg = e.message.clone();
            with_db(app, |conn| {
                finish_sync_log(conn, log_id, &finished, "cancelled", Some(&msg), updated)
            })?;
            return Err(e);
        }
        let inputs: Vec<String> = chunk.iter().map(|d| d.document.clone()).collect();
        let vectors = match client.embed(&inputs).await {
            Ok(v) => v,
            Err(e) => {
                let finished = now_rfc3339();
                let msg = e.message.clone();
                let status = if e.is_cancelled() {
                    "cancelled"
                } else {
                    "error"
                };
                with_db(app, |conn| {
                    finish_sync_log(conn, log_id, &finished, status, Some(&msg), updated)
                })?;
                emit_progress(
                    app,
                    progress("embed", processed, total, msg.clone(), Some(msg)),
                );
                return Err(e);
            }
        };

        with_db(app, |conn| {
            for (doc, embedding) in chunk.iter().zip(vectors.iter()) {
                upsert_embedding(
                    conn,
                    doc.repo_id,
                    embedding,
                    &doc.content_hash,
                    &model,
                    dimension,
                )?;
                updated += 1;
                processed += 1;
            }
            Ok(())
        })?;

        emit_progress(
            app,
            progress(
                "embed",
                processed,
                total,
                format!("Embedded {processed}/{total}"),
                None,
            ),
        );
    }

    let finished = now_rfc3339();
    with_db(app, |conn| {
        finish_sync_log(conn, log_id, &finished, "ok", None, updated)
    })?;
    emit_progress(
        app,
        progress("embed", processed, total, "Embedding complete", None),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::StarredRepo;
    use crate::services::store::open_and_migrate;
    use crate::services::sync::apply_full_diff;
    use std::time::{SystemTime, UNIX_EPOCH};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_conn() -> Connection {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("starboard_embed_{nanos}.db"));
        open_and_migrate(&path).expect("migrate")
    }

    fn sample(id: i64, name: &str, description: &str) -> StarredRepo {
        StarredRepo {
            id,
            full_name: format!("owner/{name}"),
            owner: "owner".into(),
            name: name.into(),
            description: Some(description.into()),
            language: Some("Rust".into()),
            topics: "[\"llm\",\"agents\"]".into(),
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
            starred_at: "2024-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn document_and_hash_are_stable() {
        let doc = build_document(
            "owner/mem0",
            Some("long-term memory for AI agents"),
            "[\"ai\",\"memory\"]",
            Some("Stores conversation memory"),
        );
        assert!(doc.contains("owner/mem0"));
        assert!(doc.contains("Topics: ai, memory"));
        let h1 = content_hash(&doc);
        let h2 = content_hash(&doc);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn staleness_detects_missing_and_changed_content() {
        let conn = test_conn();
        apply_full_diff(
            &conn,
            &[
                sample(1, "mem0", "agent memory"),
                sample(2, "langchain", "llm framework"),
            ],
        )
        .expect("seed");

        let stale = list_stale_or_missing(&conn).expect("stale");
        assert_eq!(stale.len(), 2);

        let dim = 768;
        let fake = vec![0.01_f32; dim as usize];
        let hash = stale[0].content_hash.clone();
        upsert_embedding(&conn, 1, &fake, &hash, "nomic-embed-text", dim).expect("upsert");

        let stale2 = list_stale_or_missing(&conn).expect("stale2");
        assert_eq!(stale2.len(), 1);
        assert_eq!(stale2[0].repo_id, 2);

        // Change description → content hash changes → repo 1 becomes stale again.
        conn.execute(
            "UPDATE repos SET description = 'changed description' WHERE id = 1",
            [],
        )
        .expect("update");
        refresh_document_hash(&conn, 1).expect("hash");
        let stale3 = list_stale_or_missing(&conn).expect("stale3");
        assert_eq!(stale3.len(), 2);
        assert!(stale3.iter().any(|d| d.repo_id == 1));
    }

    #[tokio::test]
    async fn embed_pipeline_integration_against_wiremock() {
        let server = MockServer::start().await;
        // Two repos → one batch of 2 embeddings (768-d).
        let emb: Vec<f32> = (0..768).map(|i| (i as f32) * 0.001).collect();
        let emb2: Vec<f32> = (0..768).map(|i| 1.0 - (i as f32) * 0.001).collect();
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "models": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "model": "nomic-embed-text",
                "embeddings": [emb, emb2]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let conn = test_conn();
        apply_full_diff(
            &conn,
            &[
                sample(10, "alpha", "local llm agent memory"),
                sample(11, "beta", "vector database"),
            ],
        )
        .expect("seed");
        settings::update_settings(
            &conn,
            crate::models::UpdateSettingsRequest {
                ollama_base_url: Some(server.uri()),
                ollama_chat_model: None,
                ollama_embed_model: Some("nomic-embed-text".into()),
                embed_dimension: None,
            },
        )
        .expect("settings");

        let client = OllamaClient::for_embeddings(&settings::get_settings(&conn).expect("get"))
            .expect("client");
        let pending = list_stale_or_missing(&conn).expect("pending");
        assert_eq!(pending.len(), 2);
        let inputs: Vec<String> = pending.iter().map(|d| d.document.clone()).collect();
        let vectors = client.embed(&inputs).await.expect("embed");
        assert_eq!(vectors.len(), 2);
        for (doc, vec) in pending.iter().zip(vectors.iter()) {
            upsert_embedding(
                &conn,
                doc.repo_id,
                vec,
                &doc.content_hash,
                "nomic-embed-text",
                768,
            )
            .expect("upsert");
        }
        assert!(list_stale_or_missing(&conn).expect("done").is_empty());

        // Only re-embed the repo whose description changed.
        conn.execute(
            "UPDATE repos SET description = 'brand new description text' WHERE id = 10",
            [],
        )
        .expect("change");
        refresh_document_hash(&conn, 10).expect("hash");
        let again = list_stale_or_missing(&conn).expect("again");
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].repo_id, 10);
    }

    #[test]
    fn should_attempt_auto_embed_requires_pending_and_ollama() {
        assert!(!should_attempt_auto_embed(0, true));
        assert!(!should_attempt_auto_embed(3, false));
        assert!(!should_attempt_auto_embed(0, false));
        assert!(should_attempt_auto_embed(1, true));
        assert!(should_attempt_auto_embed(50, true));
    }

    #[tokio::test]
    async fn auto_embed_post_sync_embeds_only_stale_when_ollama_up() {
        let server = MockServer::start().await;
        let emb_fresh: Vec<f32> = (0..768).map(|i| (i as f32) * 0.001).collect();
        let emb_stale: Vec<f32> = (0..768).map(|i| 0.5 + (i as f32) * 0.0001).collect();

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "models": [] })),
            )
            .mount(&server)
            .await;
        // First call embeds both; second call (after one goes stale) embeds one.
        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "model": "nomic-embed-text",
                "embeddings": [emb_fresh.clone(), emb_stale.clone()]
            })))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "model": "nomic-embed-text",
                "embeddings": [emb_stale]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let conn = test_conn();
        // Simulate post-sync library with two new repos (all missing embeddings).
        apply_full_diff(
            &conn,
            &[
                sample(20, "new-a", "first sync add"),
                sample(21, "new-b", "first sync add"),
            ],
        )
        .expect("seed");
        settings::update_settings(
            &conn,
            crate::models::UpdateSettingsRequest {
                ollama_base_url: Some(server.uri()),
                ollama_chat_model: None,
                ollama_embed_model: Some("nomic-embed-text".into()),
                embed_dimension: None,
            },
        )
        .expect("settings");

        assert!(
            is_ollama_reachable_for_embed(&settings::get_settings(&conn).expect("settings")).await
        );
        let n = auto_embed_stale_if_reachable(&conn)
            .await
            .expect("auto embed");
        assert_eq!(n, 2);
        assert!(list_stale_or_missing(&conn).expect("none").is_empty());

        // Post-sync description change → only that repo re-embeds.
        conn.execute(
            "UPDATE repos SET description = 'updated after sync' WHERE id = 20",
            [],
        )
        .expect("update");
        refresh_document_hash(&conn, 20).expect("hash");
        let stale = list_stale_or_missing(&conn).expect("stale");
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].repo_id, 20);

        let n2 = auto_embed_stale_if_reachable(&conn)
            .await
            .expect("re-embed stale");
        assert_eq!(n2, 1);
        assert!(list_stale_or_missing(&conn).expect("caught up").is_empty());
    }

    #[tokio::test]
    async fn auto_embed_noop_when_ollama_unreachable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        // Must never hit /api/embed when health fails.
        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": []
            })))
            .expect(0)
            .mount(&server)
            .await;

        let conn = test_conn();
        apply_full_diff(&conn, &[sample(30, "lonely", "needs embed")]).expect("seed");
        settings::update_settings(
            &conn,
            crate::models::UpdateSettingsRequest {
                ollama_base_url: Some(server.uri()),
                ollama_chat_model: None,
                ollama_embed_model: Some("nomic-embed-text".into()),
                embed_dimension: None,
            },
        )
        .expect("settings");

        let settings = settings::get_settings(&conn).expect("get");
        assert!(!is_ollama_reachable_for_embed(&settings).await);
        let pending = list_stale_or_missing(&conn).expect("pending").len();
        assert!(pending > 0);
        assert!(!should_attempt_auto_embed(
            pending,
            is_ollama_reachable_for_embed(&settings).await
        ));

        let n = auto_embed_stale_if_reachable(&conn)
            .await
            .expect("silent skip");
        assert_eq!(n, 0);
        assert_eq!(
            list_stale_or_missing(&conn).expect("still pending").len(),
            1
        );
    }

    #[test]
    fn embed_status_is_cheap_sql_after_description_change() {
        let conn = test_conn();
        apply_full_diff(&conn, &[sample(1, "mem0", "agent memory")]).expect("seed");
        let stale = list_stale_or_missing(&conn).expect("stale");
        let fake = vec![0.01_f32; 768];
        upsert_embedding(
            &conn,
            1,
            &fake,
            &stale[0].content_hash,
            "nomic-embed-text",
            768,
        )
        .expect("embed");
        let (total, embedded, missing, stale_n) = embed_coverage_counts(&conn).expect("counts");
        assert_eq!((total, embedded, missing, stale_n), (1, 1, 0, 0));

        conn.execute(
            "UPDATE repos SET description = 'readme changed later' WHERE id = 1",
            [],
        )
        .expect("desc");
        refresh_document_hash(&conn, 1).expect("hash");
        let (total2, embedded2, missing2, stale2) = embed_coverage_counts(&conn).expect("counts2");
        assert_eq!((total2, embedded2, missing2, stale2), (1, 1, 0, 1));
        assert_eq!(list_stale_or_missing(&conn).expect("stale").len(), 1);
    }

    #[test]
    fn upsert_embedding_rolls_back_meta_when_vector_insert_fails() {
        let conn = test_conn();
        apply_full_diff(&conn, &[sample(1, "x", "y")]).expect("seed");
        let err = crate::services::store::with_tx(&conn, |tx| {
            tx.execute("DELETE FROM repo_embedding_meta WHERE repo_id = 1", [])?;
            Err::<(), _>(AppError::db("injected embed failure"))
        })
        .expect_err("fail");
        assert_eq!(err.message, "injected embed failure");
    }
}
