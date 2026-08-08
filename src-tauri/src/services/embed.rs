use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use time::OffsetDateTime;

use crate::error::{AppError, AppResult};
use crate::models::{EmbedProgress, EmbedStatus};
use crate::services::ollama::OllamaClient;
use crate::services::settings;
use crate::services::store::DbState;

/// Repos per `/api/embed` call (task: batch e.g. 32).
const EMBED_BATCH_SIZE: usize = 32;

pub struct EmbedState {
    pub running: AtomicBool,
    pub last_error: Mutex<Option<String>>,
}

impl Default for EmbedState {
    fn default() -> Self {
        Self {
            running: AtomicBool::new(false),
            last_error: Mutex::new(None),
        }
    }
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

/// Repos that lack an embedding row or whose stored content_hash is stale.
pub fn list_stale_or_missing(conn: &Connection) -> AppResult<Vec<EmbedDoc>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.full_name, r.description, r.topics, r.readme_excerpt,
                m.content_hash
         FROM repos r
         LEFT JOIN repo_embedding_meta m ON m.repo_id = r.id
         WHERE r.unstarred = 0
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
        let (id, full_name, description, topics, readme, stored_hash) = row?;
        let document = build_document(
            &full_name,
            description.as_deref(),
            &topics,
            readme.as_deref(),
        );
        let hash = content_hash(&document);
        let needs = match stored_hash {
            None => true,
            Some(h) => h != hash,
        };
        if needs {
            out.push(EmbedDoc {
                repo_id: id,
                document,
                content_hash: hash,
            });
        }
    }
    Ok(out)
}

pub fn get_embed_status(conn: &Connection, state: &EmbedState) -> AppResult<EmbedStatus> {
    let settings = settings::get_settings(conn)?;
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
    let stale = list_stale_or_missing(conn)?.len() as i64;
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
        stale_or_missing: stale,
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
    // Replace any existing vector row, then refresh meta.
    conn.execute(
        "DELETE FROM repo_embeddings WHERE repo_id = ?1",
        params![repo_id],
    )?;
    conn.execute(
        "INSERT INTO repo_embeddings(repo_id, embedding) VALUES (?1, ?2)",
        params![repo_id, bytes],
    )?;
    let embedded_at = now_rfc3339();
    conn.execute(
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
    if let Ok(mut guard) = embed_state.last_error.lock() {
        *guard = None;
    }

    let result = run_embed_pipeline_inner(&app).await;

    embed_state.running.store(false, Ordering::SeqCst);
    if let Err(ref e) = result {
        if let Ok(mut guard) = embed_state.last_error.lock() {
            *guard = Some(e.message.clone());
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
        let inputs: Vec<String> = chunk.iter().map(|d| d.document.clone()).collect();
        let vectors = match client.embed(&inputs).await {
            Ok(v) => v,
            Err(e) => {
                let finished = now_rfc3339();
                let msg = e.message.clone();
                with_db(app, |conn| {
                    finish_sync_log(conn, log_id, &finished, "error", Some(&msg), updated)
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
        let again = list_stale_or_missing(&conn).expect("again");
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].repo_id, 10);
    }
}
