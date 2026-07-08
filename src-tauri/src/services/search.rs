use rusqlite::{params_from_iter, Connection};

use crate::error::AppResult;
use crate::models::{RepoListResult, SearchReposRequest};
use crate::services::repos::{self, build_filter_clause, order_by_clause};

const DEFAULT_LIMIT: i64 = 5000;
const MAX_LIMIT: i64 = 10_000;

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

pub fn search_repos(conn: &Connection, req: SearchReposRequest) -> AppResult<RepoListResult> {
    let query = req.query.trim();
    if query.is_empty() {
        return repos::list_repos(
            conn,
            crate::models::ListReposRequest {
                filters: req.filters,
                sort: req.sort,
                sort_desc: req.sort_desc,
                limit: req.limit,
                offset: req.offset,
            },
        );
    }

    let Some(match_query) = build_fts_match_query(query) else {
        return Ok(RepoListResult {
            items: Vec::new(),
            total: 0,
        });
    };

    let filters = req.filters.unwrap_or_default();
    let sort = req.sort.unwrap_or_default();
    let sort_desc = req.sort_desc.unwrap_or(true);
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = req.offset.unwrap_or(0).max(0);

    // Don't add fts.rowid join predicate — the JOIN already ties tables.
    let (filter_sql, mut bind) = build_filter_clause(&filters, None)?;
    // FTS query is the first bind (table-valued function argument).
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
    let rows = stmt.query_map(params_from_iter(params.iter()), |row| {
        let topics_raw: Option<String> = row.get(9)?;
        Ok(crate::models::RepoSummary {
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
    })?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }

    Ok(RepoListResult { items, total })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ListReposRequest, RepoFilters, RepoSort, StarredRepo};
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
}
