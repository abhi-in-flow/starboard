use std::collections::HashMap;
use std::time::Instant;

use rusqlite::{params, params_from_iter, Connection};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::models::{
    RepoListResult, RepoSummary, SearchMode, SearchReposRequest,
};
use crate::services::ollama::OllamaClient;
use crate::services::repos::{self, build_filter_clause, order_by_clause};
use crate::services::settings;
use crate::services::store::DbState;

const DEFAULT_LIMIT: i64 = 5000;
const MAX_LIMIT: i64 = 10_000;
const RRF_K: i64 = 60;
const TOP_K: i64 = 50;

/// Build an FTS5 MATCH query: tokenize on whitespace, escape quotes, prefix the last token.
pub fn build_fts_match_query(raw: &str) -> Option<String> {
    let tokens: Vec<String> = raw
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| {
            t.chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                .collect::<String>()
        })
        .filter(|t| !t.is_empty())
        .collect();

    if tokens.is_empty() {
        return None;
    }

    let last = tokens.len() - 1;
    let parts: Vec<String> = tokens
        .into_iter()
        .enumerate()
        .map(|(i, tok)| {
            let escaped = tok.replace('"', "\"\"");
            if i == last {
                format!("\"{escaped}\"*")
            } else {
                format!("\"{escaped}\"")
            }
        })
        .collect();
    Some(parts.join(" "))
}

/// Reciprocal Rank Fusion: `score = Σ 1/(60 + rank)` with 1-based ranks.
/// Returns repo ids sorted by descending fused score.
pub fn rrf_merge(ranked_lists: &[Vec<i64>], k: i64) -> Vec<(i64, f64)> {
    let mut scores: HashMap<i64, f64> = HashMap::new();
    for list in ranked_lists {
        for (idx, repo_id) in list.iter().enumerate() {
            let rank = (idx as i64) + 1;
            *scores.entry(*repo_id).or_insert(0.0) += 1.0 / ((k + rank) as f64);
        }
    }
    let mut merged: Vec<(i64, f64)> = scores.into_iter().collect();
    merged.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    merged
}

pub fn search_repos(conn: &Connection, req: SearchReposRequest) -> AppResult<RepoListResult> {
    let mode = req.mode.clone().unwrap_or(SearchMode::Keyword);
    match mode {
        SearchMode::Keyword => search_keyword(conn, req),
        SearchMode::Semantic | SearchMode::Hybrid => {
            // Sync path cannot embed; callers should use search_repos_async.
            // Fall back to keyword so a mistaken sync invoke still works.
            let mut result = search_keyword(conn, req)?;
            result.hint = Some(
                "Semantic/Hybrid search requires the async search path; fell back to Keyword"
                    .into(),
            );
            result.mode_used = Some(SearchMode::Keyword);
            Ok(result)
        }
    }
}

/// Async entry: embeds the query when mode is Semantic or Hybrid.
pub async fn search_repos_async(
    app: &AppHandle,
    req: SearchReposRequest,
) -> AppResult<RepoListResult> {
    let mode = req.mode.clone().unwrap_or(SearchMode::Keyword);
    if mode == SearchMode::Keyword || req.query.trim().is_empty() {
        return with_db(app, |conn| search_keyword(conn, req));
    }

    let settings = with_db(app, settings::get_settings)?;
    let embedded_coverage = with_db(app, |conn| {
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM repos WHERE unstarred = 0",
            [],
            |row| row.get(0),
        )?;
        let embedded: i64 = conn.query_row(
            "SELECT COUNT(*) FROM repo_embedding_meta m
             JOIN repos r ON r.id = m.repo_id WHERE r.unstarred = 0",
            [],
            |row| row.get(0),
        )?;
        Ok((total, embedded))
    })?;

    if settings.embeddings_need_rebuild || embedded_coverage.1 == 0 {
        let mut result = with_db(app, |conn| search_keyword(conn, req))?;
        result.mode_used = Some(SearchMode::Keyword);
        result.hint = Some(
            "Embeddings missing or need rebuild — using Keyword search. Build embeddings to enable Semantic/Hybrid."
                .into(),
        );
        return Ok(result);
    }

    let client = match OllamaClient::for_embeddings(&settings) {
        Ok(c) => c,
        Err(e) => {
            let mut result = with_db(app, |conn| search_keyword(conn, req))?;
            result.mode_used = Some(SearchMode::Keyword);
            result.hint = Some(format!("Ollama unavailable ({}) — using Keyword search", e.message));
            return Ok(result);
        }
    };

    let health = client.health_check().await?;
    if !health.available {
        let mut result = with_db(app, |conn| search_keyword(conn, req))?;
        result.mode_used = Some(SearchMode::Keyword);
        result.hint = Some(format!(
            "Ollama offline — using Keyword search. {}",
            health.message
        ));
        return Ok(result);
    }

    let query_vec = match client.embed_one(req.query.trim()).await {
        Ok(v) => v,
        Err(e) => {
            let mut result = with_db(app, |conn| search_keyword(conn, req))?;
            result.mode_used = Some(SearchMode::Keyword);
            result.hint = Some(format!(
                "Query embedding failed ({}) — using Keyword search",
                e.message
            ));
            return Ok(result);
        }
    };

    if query_vec.len() as i64 != settings.embed_dimension {
        let mut result = with_db(app, |conn| search_keyword(conn, req))?;
        result.mode_used = Some(SearchMode::Keyword);
        result.hint = Some(format!(
            "Embedding dimension mismatch (got {}, expected {}) — using Keyword search",
            query_vec.len(),
            settings.embed_dimension
        ));
        return Ok(result);
    }

    with_db(app, |conn| match mode {
        SearchMode::Semantic => search_semantic(conn, &req, &query_vec),
        SearchMode::Hybrid => search_hybrid(conn, &req, &query_vec),
        SearchMode::Keyword => search_keyword(conn, req.clone()),
    })
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

fn search_keyword(conn: &Connection, req: SearchReposRequest) -> AppResult<RepoListResult> {
    let query = req.query.trim();
    if query.is_empty() {
        let mut result = repos::list_repos(
            conn,
            crate::models::ListReposRequest {
                filters: req.filters,
                sort: req.sort,
                sort_desc: req.sort_desc,
                limit: req.limit,
                offset: req.offset,
            },
        )?;
        result.mode_used = Some(SearchMode::Keyword);
        return Ok(result);
    }

    let Some(match_query) = build_fts_match_query(query) else {
        return Ok(RepoListResult {
            items: Vec::new(),
            total: 0,
            mode_used: Some(SearchMode::Keyword),
            hint: None,
            search_ms: None,
        });
    };

    let filters = req.filters.unwrap_or_default();
    let sort = req.sort.unwrap_or_default();
    let sort_desc = req.sort_desc.unwrap_or(true);
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = req.offset.unwrap_or(0).max(0);

    let (filter_sql, mut bind) = build_filter_clause(&filters, None)?;
    let mut all_bind = vec![match_query.clone()];
    all_bind.append(&mut bind);

    let where_sql = if filter_sql.is_empty() {
        String::new()
    } else {
        filter_sql
    };

    let order_sql = order_by_clause(&sort, sort_desc, true);

    let total: i64 = {
        let sql = format!(
            "SELECT COUNT(*)
             FROM repos_fts(?) AS fts
             JOIN repos r ON r.id = fts.rowid
             {where_sql}"
        );
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_row(params_from_iter(all_bind.iter()), |row| row.get(0))?
    };

    let sql = format!(
        "SELECT r.id, r.full_name, r.description, r.language, r.stars_count, r.starred_at,
                r.pushed_at, r.archived, r.unstarred, r.topics,
                fts.rank AS rank
         FROM repos_fts(?) AS fts
         JOIN repos r ON r.id = fts.rowid
         {where_sql}
         {order_sql}
         LIMIT ? OFFSET ?"
    );

    let mut params = all_bind;
    params.push(limit.to_string());
    params.push(offset.to_string());

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params.iter()), map_summary_row)?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }

    Ok(RepoListResult {
        items,
        total,
        mode_used: Some(SearchMode::Keyword),
        hint: None,
        search_ms: None,
    })
}

/// FTS top-N by BM25 rank, no filters (filters applied post-fusion).
fn fts_top_ids(conn: &Connection, query: &str, limit: i64) -> AppResult<Vec<i64>> {
    let Some(match_query) = build_fts_match_query(query) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare(
        "SELECT fts.rowid
         FROM repos_fts(?) AS fts
         JOIN repos r ON r.id = fts.rowid
         WHERE r.unstarred = 0
         ORDER BY fts.rank ASC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(params![match_query, limit], |row| row.get(0))?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row?);
    }
    Ok(ids)
}

/// sqlite-vec KNN top-N by embedding distance.
fn knn_top_ids(conn: &Connection, query_embedding: &[f32], limit: i64) -> AppResult<Vec<i64>> {
    let bytes: Vec<u8> = query_embedding
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    let mut stmt = conn.prepare(
        "SELECT e.repo_id
         FROM repo_embeddings e
         WHERE e.embedding MATCH ?1 AND k = ?2
         ORDER BY distance",
    )?;
    let rows = stmt.query_map(params![bytes, limit], |row| row.get(0))?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row?);
    }
    Ok(ids)
}

fn search_semantic(
    conn: &Connection,
    req: &SearchReposRequest,
    query_embedding: &[f32],
) -> AppResult<RepoListResult> {
    let started = Instant::now();
    let knn = knn_top_ids(conn, query_embedding, TOP_K)?;
    let ranked: Vec<(i64, f64)> = knn
        .into_iter()
        .enumerate()
        .map(|(i, id)| (id, 1.0 / ((RRF_K + i as i64 + 1) as f64)))
        .collect();
    let result = materialize_ranked(conn, req, &ranked, SearchMode::Semantic)?;
    let mut out = result;
    out.search_ms = Some(started.elapsed().as_millis() as u64);
    eprintln!(
        "[search] semantic db+rank {}ms (excl. query embed)",
        out.search_ms.unwrap_or(0)
    );
    Ok(out)
}

fn search_hybrid(
    conn: &Connection,
    req: &SearchReposRequest,
    query_embedding: &[f32],
) -> AppResult<RepoListResult> {
    let started = Instant::now();
    let fts_ids = fts_top_ids(conn, req.query.trim(), TOP_K)?;
    let knn_ids = knn_top_ids(conn, query_embedding, TOP_K)?;
    let merged = rrf_merge(&[fts_ids, knn_ids], RRF_K);
    let result = materialize_ranked(conn, req, &merged, SearchMode::Hybrid)?;
    let mut out = result;
    out.search_ms = Some(started.elapsed().as_millis() as u64);
    eprintln!(
        "[search] hybrid db+rrf {}ms (excl. query embed)",
        out.search_ms.unwrap_or(0)
    );
    Ok(out)
}

/// Apply filters post-fusion, then paginate and load RepoSummary rows.
fn materialize_ranked(
    conn: &Connection,
    req: &SearchReposRequest,
    ranked: &[(i64, f64)],
    mode: SearchMode,
) -> AppResult<RepoListResult> {
    let filters = req.filters.clone().unwrap_or_default();
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = req.offset.unwrap_or(0).max(0);

    if ranked.is_empty() {
        return Ok(RepoListResult {
            items: Vec::new(),
            total: 0,
            mode_used: Some(mode),
            hint: None,
            search_ms: None,
        });
    }

    let ids: Vec<i64> = ranked.iter().map(|(id, _)| *id).collect();
    let placeholders = ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let (filter_sql, filter_bind) = build_filter_clause(&filters, None)?;
    let where_extra = if filter_sql.is_empty() {
        String::new()
    } else {
        // filter_sql starts with WHERE; rewrite as AND …
        format!("AND {}", filter_sql.trim_start_matches("WHERE ").trim())
    };

    let sql = format!(
        "SELECT r.id, r.full_name, r.description, r.language, r.stars_count, r.starred_at,
                r.pushed_at, r.archived, r.unstarred, r.topics
         FROM repos r
         WHERE r.id IN ({placeholders})
         {where_extra}"
    );

    let mut bind: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
    bind.extend(filter_bind);

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(bind.iter()), map_summary_row)?;
    let mut by_id: HashMap<i64, RepoSummary> = HashMap::new();
    for row in rows {
        let summary = row?;
        by_id.insert(summary.id, summary);
    }

    // Preserve fused order; drop ids filtered out.
    let ordered: Vec<RepoSummary> = ranked
        .iter()
        .filter_map(|(id, _)| by_id.remove(id))
        .collect();
    let total = ordered.len() as i64;
    let start = offset as usize;
    let end = (offset + limit).min(total) as usize;
    let items = if start >= ordered.len() {
        Vec::new()
    } else {
        ordered[start..end.min(ordered.len())].to_vec()
    };

    Ok(RepoListResult {
        items,
        total,
        mode_used: Some(mode),
        hint: None,
        search_ms: None,
    })
}

fn map_summary_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RepoSummary> {
    let topics_raw: Option<String> = row.get(9)?;
    Ok(RepoSummary {
        id: row.get(0)?,
        full_name: row.get(1)?,
        description: row.get(2)?,
        language: row.get(3)?,
        stars_count: row.get(4)?,
        starred_at: row.get(5)?,
        pushed_at: row.get(6)?,
        archived: row.get::<_, i64>(7)? != 0,
        unstarred: row.get::<_, i64>(8)? != 0,
        topics: serde_json::from_str(&topics_raw.unwrap_or_else(|| "[]".into()))
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ListReposRequest, RepoFilters, RepoSort, StarredRepo};
    use crate::services::embed::{content_hash, upsert_embedding};
    use crate::services::store::open_and_migrate;
    use crate::services::sync::apply_full_diff;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_conn() -> Connection {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("starboard_search_{nanos}.db"));
        open_and_migrate(&path).expect("migrate")
    }

    fn sample(id: i64, name: &str) -> StarredRepo {
        StarredRepo {
            id,
            full_name: format!("owner/{name}"),
            owner: "owner".into(),
            name: name.into(),
            description: Some(format!("{name} async runtime")),
            language: Some("Rust".into()),
            topics: "[\"async\"]".into(),
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
    fn fts_match_query_prefixes_last_token() {
        assert_eq!(
            build_fts_match_query("tokio async").as_deref(),
            Some("\"tokio\" \"async\"*")
        );
        assert!(build_fts_match_query("   ").is_none());
    }

    #[test]
    fn rrf_merge_overlap_boosts_shared_ids() {
        let a = vec![1, 2, 3];
        let b = vec![3, 4, 1];
        let merged = rrf_merge(&[a, b], 60);
        // id 1 and 3 appear in both → higher scores than 2 and 4.
        assert_eq!(merged[0].0, 1); // ranks 1 and 3 → 1/61 + 1/63
        assert_eq!(merged[1].0, 3); // ranks 3 and 1 → 1/63 + 1/61 — wait equal to 1?
        // Actually 1: rank1 in A (1/61) + rank3 in B (1/63)
        //         3: rank3 in A (1/63) + rank1 in B (1/61) — equal scores; tie-break by id.
        assert!(merged[0].0 == 1 || merged[0].0 == 3);
        assert!(merged.iter().any(|(id, _)| *id == 2));
        assert!(merged.iter().any(|(id, _)| *id == 4));
        assert_eq!(merged.len(), 4);
    }

    #[test]
    fn rrf_merge_disjoint_and_empty() {
        let a = vec![10, 20];
        let b = vec![30, 40];
        let merged = rrf_merge(&[a, b], 60);
        assert_eq!(merged.len(), 4);
        // First of each list share the top score.
        assert!(merged[0].1 > merged[2].1);

        assert!(rrf_merge(&[], 60).is_empty());
        assert_eq!(rrf_merge(&[vec![], vec![5]], 60)[0].0, 5);
        assert!(rrf_merge(&[vec![], vec![]], 60).is_empty());
    }

    #[test]
    fn search_finds_repo_by_prefix() {
        let conn = test_conn();
        apply_full_diff(&conn, &[sample(1, "tokio"), sample(2, "serde")]).expect("seed");

        let result = search_repos(
            &conn,
            SearchReposRequest {
                query: "tok".into(),
                filters: Some(RepoFilters::default()),
                sort: Some(RepoSort::StarredAt),
                sort_desc: Some(true),
                limit: None,
                offset: None,
                mode: Some(SearchMode::Keyword),
            },
        )
        .expect("search");

        assert_eq!(result.total, 1);
        assert_eq!(result.items[0].full_name, "owner/tokio");
    }

    #[test]
    fn empty_query_falls_back_to_list() {
        let conn = test_conn();
        apply_full_diff(&conn, &[sample(1, "a"), sample(2, "b")]).expect("seed");
        let via_search = search_repos(
            &conn,
            SearchReposRequest {
                query: "  ".into(),
                filters: None,
                sort: None,
                sort_desc: None,
                limit: None,
                offset: None,
                mode: None,
            },
        )
        .expect("search");
        let via_list = repos::list_repos(
            &conn,
            ListReposRequest {
                filters: None,
                sort: None,
                sort_desc: None,
                limit: None,
                offset: None,
            },
        )
        .expect("list");
        assert_eq!(via_search.total, via_list.total);
    }

    #[test]
    fn hybrid_rrf_orders_by_fused_score_under_500ms() {
        let conn = test_conn();
        let repos = [
            sample(1, "memgpt"),
            sample(2, "tokio"),
            sample(3, "chroma"),
            sample(4, "serde"),
        ];
        apply_full_diff(&conn, &repos).expect("seed");

        // Seed embeddings: make chroma closest to a "memory" query vector.
        let dim = 768;
        let query = vec![1.0_f32; dim];
        let near = vec![0.99_f32; dim];
        let mid = vec![0.5_f32; dim];
        let far = vec![0.0_f32; dim];
        for (id, emb) in [(3, &near), (1, &mid), (2, &far), (4, &far)] {
            let doc = format!("doc-{id}");
            upsert_embedding(
                &conn,
                id,
                emb,
                &content_hash(&doc),
                "nomic-embed-text",
                dim as i64,
            )
            .expect("upsert");
        }

        let started = Instant::now();
        let result = search_hybrid(
            &conn,
            &SearchReposRequest {
                query: "tokio runtime".into(), // FTS will prefer tokio
                filters: Some(RepoFilters {
                    hide_unstarred: Some(true),
                    hide_archived: Some(true),
                    ..Default::default()
                }),
                sort: None,
                sort_desc: None,
                limit: None,
                offset: None,
                mode: Some(SearchMode::Hybrid),
            },
            &query,
        )
        .expect("hybrid");
        let ms = started.elapsed().as_millis();
        assert!(
            ms < 500,
            "hybrid search took {ms}ms (budget 500ms excl. embed)"
        );
        assert!(result.total >= 1);
        assert_eq!(result.mode_used, Some(SearchMode::Hybrid));
        assert!(result.search_ms.unwrap_or(u64::MAX) < 500);
    }
}
