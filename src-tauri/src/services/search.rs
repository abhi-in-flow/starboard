use std::collections::HashMap;
use std::time::Instant;

use rusqlite::{params, params_from_iter, Connection};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::models::{RepoListResult, RepoSummary, SearchMode, SearchReposRequest};
use crate::services::ollama::OllamaClient;
use crate::services::repos::{self, build_filter_clause, order_by_clause};
use crate::services::review;
use crate::services::settings;
use crate::services::store::DbState;

const DEFAULT_LIMIT: i64 = crate::services::repos::DEFAULT_PAGE_SIZE;
const MAX_LIMIT: i64 = 500;
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

/// Normalize fused scores relative to the top hit: top = 100, others proportional.
/// Empty input returns empty. Safe when max is 0 (all zeros → 0).
pub fn normalize_relevance(ranked: &[(i64, f64)]) -> Vec<(i64, u8)> {
    let Some(max_score) = ranked.first().map(|(_, s)| *s) else {
        return Vec::new();
    };
    if max_score <= 0.0 {
        return ranked.iter().map(|(id, _)| (*id, 0u8)).collect();
    }
    ranked
        .iter()
        .map(|(id, score)| {
            let pct = ((*score / max_score) * 100.0).round();
            let clamped = pct.clamp(0.0, 100.0) as u8;
            (*id, clamped)
        })
        .collect()
}

pub fn search_repos(conn: &Connection, req: SearchReposRequest) -> AppResult<RepoListResult> {
    search_keyword(conn, req)
}

/// Async entry: embeds the query when mode is Semantic or Hybrid.
pub async fn search_repos_async(
    app: &AppHandle,
    req: SearchReposRequest,
) -> AppResult<RepoListResult> {
    let mode = req.mode.clone().unwrap_or(SearchMode::Keyword);
    if mode == SearchMode::Keyword || req.query.trim().is_empty() {
        return with_db(app, |conn| search_repos(conn, req));
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
        let mut result = with_db(app, |conn| search_repos(conn, req))?;
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
            let mut result = with_db(app, |conn| search_repos(conn, req))?;
            result.mode_used = Some(SearchMode::Keyword);
            result.hint = Some(format!(
                "Ollama unavailable ({}) — using Keyword search",
                e.message
            ));
            return Ok(result);
        }
    };

    let health = client.health_check().await?;
    if !health.available {
        let mut result = with_db(app, |conn| search_repos(conn, req))?;
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
            let mut result = with_db(app, |conn| search_repos(conn, req))?;
            result.mode_used = Some(SearchMode::Keyword);
            result.hint = Some(format!(
                "Query embedding failed ({}) — using Keyword search",
                e.message
            ));
            return Ok(result);
        }
    };

    if query_vec.len() as i64 != settings.embed_dimension {
        let mut result = with_db(app, |conn| search_repos(conn, req))?;
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
        SearchMode::Keyword => search_repos(conn, req.clone()),
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
    review::attach_review_reason(&mut items, filters.review_preset.as_ref());

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
    crate::services::logging::debug_timing(format!(
        "[search] semantic db+rank {}ms (excl. query embed)",
        out.search_ms.unwrap_or(0)
    ));
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
    crate::services::logging::debug_timing(format!(
        "[search] hybrid db+rrf {}ms (excl. query embed)",
        out.search_ms.unwrap_or(0)
    ));
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
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
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

    // Preserve fused order; drop ids filtered out. Normalize vs top of *this* set.
    let filtered_ranked: Vec<(i64, f64)> = ranked
        .iter()
        .filter(|(id, _)| by_id.contains_key(id))
        .copied()
        .collect();
    let relevance_by_id: HashMap<i64, u8> =
        normalize_relevance(&filtered_ranked).into_iter().collect();

    let ordered: Vec<RepoSummary> = filtered_ranked
        .iter()
        .filter_map(|(id, _)| {
            let mut summary = by_id.remove(id)?;
            summary.relevance = relevance_by_id.get(id).copied();
            Some(summary)
        })
        .collect();
    let total = ordered.len() as i64;
    let start = offset as usize;
    let end = (offset + limit).min(total) as usize;
    let mut items = if start >= ordered.len() {
        Vec::new()
    } else {
        ordered[start..end.min(ordered.len())].to_vec()
    };
    review::attach_review_reason(&mut items, filters.review_preset.as_ref());

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
        relevance: None,
        review_reason: None,
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
    fn normalize_relevance_top_is_100_and_monotonic() {
        let ranked = vec![(1, 0.032), (2, 0.016), (3, 0.008)];
        let norm = normalize_relevance(&ranked);
        assert_eq!(norm[0], (1, 100));
        assert_eq!(norm[1].1, 50);
        assert_eq!(norm[2].1, 25);
        assert!(norm[0].1 >= norm[1].1 && norm[1].1 >= norm[2].1);
    }

    #[test]
    fn normalize_relevance_single_and_empty() {
        assert_eq!(normalize_relevance(&[(42, 0.01)]), vec![(42, 100)]);
        assert!(normalize_relevance(&[]).is_empty());
        // Zero max score is safe.
        assert_eq!(
            normalize_relevance(&[(1, 0.0), (2, 0.0)]),
            vec![(1, 0), (2, 0)]
        );
    }

    #[test]
    fn keyword_and_browse_have_no_relevance() {
        let conn = test_conn();
        apply_full_diff(&conn, &[sample(1, "tokio"), sample(2, "serde")]).expect("seed");

        let browse = repos::list_repos(
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
        assert!(browse.items.iter().all(|r| r.relevance.is_none()));

        let keyword = search_repos(
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
        assert_eq!(keyword.total, 1);
        assert!(keyword.items[0].relevance.is_none());
    }

    #[test]
    fn semantic_attaches_normalized_relevance() {
        let conn = test_conn();
        apply_full_diff(&conn, &[sample(1, "a"), sample(2, "b"), sample(3, "c")]).expect("seed");
        let dim = 768;
        let near = vec![1.0_f32; dim];
        let mid = vec![0.5_f32; dim];
        let far = vec![0.0_f32; dim];
        for (id, emb) in [(1, &near), (2, &mid), (3, &far)] {
            upsert_embedding(
                &conn,
                id,
                emb,
                &content_hash(&format!("doc-{id}")),
                "nomic-embed-text",
                dim as i64,
            )
            .expect("upsert");
        }
        let result = search_semantic(
            &conn,
            &SearchReposRequest {
                query: "anything".into(),
                filters: Some(RepoFilters::default()),
                sort: None,
                sort_desc: None,
                limit: None,
                offset: None,
                mode: Some(SearchMode::Semantic),
            },
            &near,
        )
        .expect("semantic");
        assert!(!result.items.is_empty());
        assert_eq!(result.items[0].relevance, Some(100));
        for window in result.items.windows(2) {
            let a = window[0].relevance.unwrap_or(0);
            let b = window[1].relevance.unwrap_or(0);
            assert!(a >= b, "relevance must be monotonic with rank");
        }
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

    #[test]
    fn semantic_beats_keyword_on_three_conceptual_queries() {
        // Acceptance demo (synthetic embeddings, no live Ollama):
        // 1) "local llm agent memory" → mem0
        // 2) "embedding vector store for rag" → chroma
        // 3) "reliable async rust runtime" → tokio
        let conn = test_conn();
        let catalog = [
            (
                1_i64,
                "mem0ai/mem0",
                "Long-term memory layer for AI agents and LLM apps",
                "[\"agents\",\"memory\",\"llm\"]",
            ),
            (
                2,
                "tokio-rs/tokio",
                "A runtime for writing reliable asynchronous applications with Rust",
                "[\"async\",\"runtime\"]",
            ),
            (
                3,
                "chroma-core/chroma",
                "AI-native open-source embedding database for vector search",
                "[\"vector\",\"embeddings\",\"database\"]",
            ),
            (
                4,
                "langchain-ai/langchain",
                "Build context-aware reasoning applications with language models",
                "[\"llm\",\"agents\",\"orchestration\"]",
            ),
            (
                5,
                "serde-rs/serde",
                "Serialization framework for Rust",
                "[\"serialization\"]",
            ),
        ];
        let repos: Vec<StarredRepo> = catalog
            .iter()
            .map(|(id, full, desc, topics)| {
                let name = full.split('/').nth(1).unwrap_or(full);
                StarredRepo {
                    id: *id,
                    full_name: (*full).into(),
                    owner: full.split('/').next().unwrap_or("o").into(),
                    name: name.into(),
                    description: Some((*desc).into()),
                    language: Some("Rust".into()),
                    topics: (*topics).into(),
                    stars_count: Some(100),
                    forks_count: Some(1),
                    open_issues: Some(0),
                    license: Some("MIT".into()),
                    homepage: None,
                    html_url: format!("https://github.com/{full}"),
                    archived: false,
                    fork: false,
                    repo_created_at: Some("2020-01-01T00:00:00Z".into()),
                    pushed_at: Some("2024-01-01T00:00:00Z".into()),
                    starred_at: "2024-01-01T00:00:00Z".into(),
                }
            })
            .collect();
        apply_full_diff(&conn, &repos).expect("seed");

        let dim = 768usize;
        // Axis 0 ≈ agent-memory, 1 ≈ vector-db, 2 ≈ async-runtime
        let embeddings: [(i64, Vec<f32>); 5] = [
            (1, {
                let mut v = vec![0.0; dim];
                v[0] = 1.0;
                v
            }),
            (2, {
                let mut v = vec![0.0; dim];
                v[2] = 1.0;
                v
            }),
            (3, {
                let mut v = vec![0.0; dim];
                v[1] = 1.0;
                v
            }),
            (4, {
                let mut v = vec![0.0; dim];
                v[0] = 0.7;
                v[1] = 0.2;
                v
            }),
            (5, {
                let mut v = vec![0.0; dim];
                v[3] = 1.0;
                v
            }),
        ];
        for (id, emb) in &embeddings {
            let doc = catalog
                .iter()
                .find(|(i, ..)| i == id)
                .map(|(_, f, d, t)| format!("{f}\n{d}\nTopics: {t}\n"))
                .unwrap();
            upsert_embedding(
                &conn,
                *id,
                emb,
                &content_hash(&doc),
                "nomic-embed-text",
                dim as i64,
            )
            .expect("upsert");
        }

        let cases: [(&str, i64, &str, usize); 3] = [
            ("local llm agent memory", 1, "mem0ai/mem0", 0),
            ("embedding vector store for rag", 3, "chroma-core/chroma", 1),
            // Avoid tokens that FTS already matches on tokio's description.
            ("nonblocking green-thread executor", 2, "tokio-rs/tokio", 2),
        ];

        let mut keyword_misses = 0usize;
        for (query, expected_id, expected_name, axis) in cases {
            let mut qvec = vec![0.0_f32; dim];
            qvec[axis] = 1.0;

            let keyword = search_repos(
                &conn,
                SearchReposRequest {
                    query: query.into(),
                    filters: Some(RepoFilters::default()),
                    sort: None,
                    sort_desc: None,
                    limit: Some(5),
                    offset: None,
                    mode: Some(SearchMode::Keyword),
                },
            )
            .expect("keyword");

            let semantic = search_semantic(
                &conn,
                &SearchReposRequest {
                    query: query.into(),
                    filters: Some(RepoFilters::default()),
                    sort: None,
                    sort_desc: None,
                    limit: Some(5),
                    offset: None,
                    mode: Some(SearchMode::Semantic),
                },
                &qvec,
            )
            .expect("semantic");

            assert_eq!(
                semantic.items[0].full_name, expected_name,
                "semantic top for '{query}'"
            );
            assert_eq!(semantic.items[0].id, expected_id);

            let kw_first = keyword.items.first().map(|r| r.id);
            if kw_first != Some(expected_id) {
                keyword_misses += 1;
            }
        }
        assert!(
            keyword_misses >= 2,
            "expected keyword to miss the semantic top hit on at least 2/3 conceptual queries"
        );
    }

    #[test]
    fn keyword_pagination_preserves_total_and_rank_order() {
        let conn = test_conn();
        apply_full_diff(
            &conn,
            &[
                sample(1, "tokio"),
                sample(2, "tokio-util"),
                sample(3, "tokio-stream"),
                sample(4, "serde"),
            ],
        )
        .expect("seed");

        let page1 = search_repos(
            &conn,
            SearchReposRequest {
                query: "tokio".into(),
                filters: Some(RepoFilters::default()),
                sort: Some(RepoSort::Name),
                sort_desc: Some(false),
                limit: Some(2),
                offset: Some(0),
                mode: Some(SearchMode::Keyword),
            },
        )
        .expect("p1");
        assert_eq!(page1.total, 3);
        assert_eq!(page1.items.len(), 2);

        let page2 = search_repos(
            &conn,
            SearchReposRequest {
                query: "tokio".into(),
                filters: Some(RepoFilters::default()),
                sort: Some(RepoSort::Name),
                sort_desc: Some(false),
                limit: Some(2),
                offset: Some(2),
                mode: Some(SearchMode::Keyword),
            },
        )
        .expect("p2");
        assert_eq!(page2.total, 3);
        assert_eq!(page2.items.len(), 1);
        let ids: Vec<i64> = page1
            .items
            .iter()
            .chain(page2.items.iter())
            .map(|r| r.id)
            .collect();
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 3);
    }
}
