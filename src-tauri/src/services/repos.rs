use rusqlite::{params_from_iter, Connection, Row};

use crate::error::{AppError, AppResult};
use crate::models::{
    FacetCount, LibraryFacets, ListReposRequest, RepoDetail, RepoFilters, RepoListResult, RepoSort,
    RepoSummary,
};

pub const DEFAULT_PAGE_SIZE: i64 = 100;
const MAX_LIMIT: i64 = 500;

pub fn list_repos(conn: &Connection, req: ListReposRequest) -> AppResult<RepoListResult> {
    let filters = req.filters.unwrap_or_default();
    let sort = req.sort.unwrap_or_default();
    let sort_desc = req.sort_desc.unwrap_or(true);
    let limit = req.limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_LIMIT);
    let offset = req.offset.unwrap_or(0).max(0);

    let (where_sql, bind) = build_filter_clause(&filters, None)?;
    let order_sql = order_by_clause(&sort, sort_desc, false);

    let total: i64 = {
        let sql = format!("SELECT COUNT(*) FROM repos r {where_sql}");
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_row(params_from_iter(bind.iter()), |row| row.get(0))?
    };

    let sql = format!(
        "SELECT r.id, r.full_name, r.description, r.language, r.stars_count, r.starred_at,
                r.pushed_at, r.archived, r.unstarred, r.topics
         FROM repos r
         {where_sql}
         {order_sql}
         LIMIT ? OFFSET ?"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut params: Vec<String> = bind;
    params.push(limit.to_string());
    params.push(offset.to_string());

    let rows = stmt.query_map(params_from_iter(params.iter()), map_summary)?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }

    Ok(RepoListResult {
        items,
        total,
        mode_used: None,
        hint: None,
        search_ms: None,
    })
}

pub fn get_repo(conn: &Connection, id: i64) -> AppResult<RepoDetail> {
    let mut stmt = conn.prepare(
        "SELECT id, full_name, owner, name, description, language, topics,
                stars_count, forks_count, open_issues, license, homepage, html_url,
                archived, fork, repo_created_at, pushed_at, starred_at,
                readme_excerpt, fetched_at, unstarred
         FROM repos WHERE id = ?1",
    )?;
    let mut rows = stmt.query([id])?;
    let Some(row) = rows.next()? else {
        return Err(AppError::new("not_found", format!("repo {id} not found")));
    };
    let mut detail = map_detail(row)?;
    detail.category_names = load_category_names(conn, id)?;
    let (category_id, category_source) = load_primary_category(conn, id)?;
    detail.category_id = category_id;
    detail.category_source = category_source;
    Ok(detail)
}

pub fn library_facets(conn: &Connection, filters: Option<RepoFilters>) -> AppResult<LibraryFacets> {
    let filters = filters.unwrap_or_default();
    // Facets ignore the active language/topic so users can switch chips.
    let mut base = filters.clone();
    base.language = None;
    base.topic = None;

    let (where_sql, bind) = build_filter_clause(&base, None)?;

    let languages = {
        let sql = format!(
            "SELECT language AS name, COUNT(*) AS count
             FROM repos r
             {where_sql}
             AND language IS NOT NULL AND language != ''
             GROUP BY language
             ORDER BY count DESC, language ASC
             LIMIT 50"
        );
        query_facets(conn, &sql, &bind)?
    };

    let topics = {
        // topics stored as JSON array text; extract via json_each when valid JSON.
        let sql = format!(
            "SELECT t.value AS name, COUNT(*) AS count
             FROM repos r, json_each(r.topics) t
             {where_sql}
             AND r.topics IS NOT NULL AND r.topics != '' AND r.topics != '[]'
             GROUP BY t.value
             ORDER BY count DESC, name ASC
             LIMIT 50"
        );
        query_facets(conn, &sql, &bind).unwrap_or_default()
    };

    Ok(LibraryFacets { languages, topics })
}

fn query_facets(conn: &Connection, sql: &str, bind: &[String]) -> AppResult<Vec<FacetCount>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params_from_iter(bind.iter()), |row| {
        Ok(FacetCount {
            name: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Primary category for a repo: manual override wins, else highest-confidence LLM pick.
fn load_primary_category(
    conn: &Connection,
    repo_id: i64,
) -> AppResult<(Option<i64>, Option<String>)> {
    let mut stmt = conn.prepare(
        "SELECT rc.category_id, rc.source
         FROM repo_categories rc
         WHERE rc.repo_id = ?1
         ORDER BY CASE WHEN rc.source = 'manual' THEN 0 ELSE 1 END,
                  rc.confidence DESC
         LIMIT 1",
    )?;
    let mut rows = stmt.query([repo_id])?;
    if let Some(row) = rows.next()? {
        Ok((row.get(0)?, row.get(1)?))
    } else {
        Ok((None, None))
    }
}

fn load_category_names(conn: &Connection, repo_id: i64) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT c.name
         FROM repo_categories rc
         JOIN categories c ON c.id = rc.category_id
         WHERE rc.repo_id = ?1
         ORDER BY CASE WHEN rc.source = 'manual' THEN 0 ELSE 1 END,
                  rc.confidence DESC,
                  c.name",
    )?;
    let rows = stmt.query_map([repo_id], |row| row.get(0))?;
    let mut names = Vec::new();
    for row in rows {
        names.push(row?);
    }
    Ok(names)
}

pub fn build_filter_clause(
    filters: &RepoFilters,
    fts_alias: Option<&str>,
) -> AppResult<(String, Vec<String>)> {
    let mut clauses = Vec::new();
    let mut bind = Vec::new();

    let hide_unstarred = filters.hide_unstarred.unwrap_or(true);
    if hide_unstarred {
        clauses.push("r.unstarred = 0".to_string());
    }

    let hide_archived = filters.hide_archived.unwrap_or(true);
    if filters.archived_only.unwrap_or(false) {
        clauses.push("r.archived = 1".to_string());
    } else if hide_archived {
        clauses.push("r.archived = 0".to_string());
    }

    if let Some(lang) = filters.language.as_ref().filter(|s| !s.is_empty()) {
        clauses.push("r.language = ?".to_string());
        bind.push(lang.clone());
    }

    if let Some(topic) = filters.topic.as_ref().filter(|s| !s.is_empty()) {
        // topics is a JSON array string; match quoted token.
        clauses.push("instr(r.topics, ?) > 0".to_string());
        bind.push(format!("\"{topic}\""));
    }

    if let Some(category_id) = filters.category_id {
        clauses.push(
            "EXISTS (
                SELECT 1 FROM repo_categories rc
                WHERE rc.repo_id = r.id AND rc.category_id = ?
             )"
            .to_string(),
        );
        bind.push(category_id.to_string());
    }

    if let Some(alias) = fts_alias {
        clauses.push(format!("{alias}.rowid = r.id"));
    }

    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };
    Ok((where_sql, bind))
}

pub fn order_by_clause(sort: &RepoSort, desc: bool, fts_rank: bool) -> String {
    let dir = if desc { "DESC" } else { "ASC" };
    // Unique id tie-breaker keeps LIMIT/OFFSET pages stable when the primary
    // key (especially starred_at) is equal. Primary sort semantics are unchanged.
    let tie = "r.id ASC";
    let secondary = match sort {
        RepoSort::StarredAt => format!("r.starred_at {dir}, {tie}"),
        RepoSort::Stars => format!("COALESCE(r.stars_count, -1) {dir}, r.full_name ASC, {tie}"),
        RepoSort::PushedAt => format!("COALESCE(r.pushed_at, '') {dir}, r.full_name ASC, {tie}"),
        RepoSort::Name => format!("r.full_name {dir}, {tie}"),
    };
    if fts_rank {
        format!("ORDER BY rank ASC, {secondary}")
    } else {
        format!("ORDER BY {secondary}")
    }
}

fn parse_topics(raw: Option<String>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default()
}

fn map_summary(row: &Row<'_>) -> rusqlite::Result<RepoSummary> {
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
        topics: parse_topics(topics_raw),
        relevance: None,
    })
}

fn map_detail(row: &Row<'_>) -> rusqlite::Result<RepoDetail> {
    let topics_raw: Option<String> = row.get(6)?;
    Ok(RepoDetail {
        id: row.get(0)?,
        full_name: row.get(1)?,
        owner: row.get(2)?,
        name: row.get(3)?,
        description: row.get(4)?,
        language: row.get(5)?,
        topics: parse_topics(topics_raw),
        stars_count: row.get(7)?,
        forks_count: row.get(8)?,
        open_issues: row.get(9)?,
        license: row.get(10)?,
        homepage: row.get(11)?,
        html_url: row.get(12)?,
        archived: row.get::<_, i64>(13)? != 0,
        fork: row.get::<_, i64>(14)? != 0,
        repo_created_at: row.get(15)?,
        pushed_at: row.get(16)?,
        starred_at: row.get(17)?,
        readme_excerpt: row.get(18)?,
        fetched_at: row.get(19)?,
        unstarred: row.get::<_, i64>(20)? != 0,
        category_names: Vec::new(),
        category_id: None,
        category_source: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::StarredRepo;
    use crate::services::store::open_and_migrate;
    use crate::services::sync::apply_full_diff;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_conn() -> Connection {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("starboard_repos_{nanos}.db"));
        open_and_migrate(&path).expect("migrate")
    }

    fn sample(id: i64, name: &str, lang: &str, stars: i64) -> StarredRepo {
        StarredRepo {
            id,
            full_name: format!("owner/{name}"),
            owner: "owner".into(),
            name: name.into(),
            description: Some(format!("{name} async toolkit")),
            language: Some(lang.into()),
            topics: "[\"rust\",\"async\"]".into(),
            stars_count: Some(stars),
            forks_count: Some(1),
            open_issues: Some(0),
            license: Some("MIT".into()),
            homepage: None,
            html_url: format!("https://github.com/owner/{name}"),
            archived: false,
            fork: false,
            repo_created_at: Some("2020-01-01T00:00:00Z".into()),
            pushed_at: Some("2024-01-01T00:00:00Z".into()),
            starred_at: format!("2024-0{id}-01T00:00:00Z"),
        }
    }

    #[test]
    fn list_filters_language_and_sorts_stars() {
        let conn = test_conn();
        apply_full_diff(
            &conn,
            &[
                sample(1, "alpha", "Rust", 10),
                sample(2, "beta", "Go", 50),
                sample(3, "gamma", "Rust", 30),
            ],
        )
        .expect("seed");

        let result = list_repos(
            &conn,
            ListReposRequest {
                filters: Some(RepoFilters {
                    language: Some("Rust".into()),
                    ..Default::default()
                }),
                sort: Some(RepoSort::Stars),
                sort_desc: Some(true),
                limit: None,
                offset: None,
            },
        )
        .expect("list");

        assert_eq!(result.total, 2);
        assert_eq!(result.items[0].full_name, "owner/gamma");
        assert_eq!(result.items[1].full_name, "owner/alpha");
    }

    #[test]
    fn get_repo_returns_detail() {
        let conn = test_conn();
        apply_full_diff(&conn, &[sample(9, "detail", "Rust", 1)]).expect("seed");
        let detail = get_repo(&conn, 9).expect("get");
        assert_eq!(detail.name, "detail");
        assert!(detail.topics.contains(&"async".into()));
    }

    #[test]
    fn pagination_is_stable_and_reports_total() {
        let conn = test_conn();
        apply_full_diff(
            &conn,
            &[
                sample(1, "alpha", "Rust", 10),
                sample(2, "beta", "Go", 50),
                sample(3, "gamma", "Rust", 30),
                sample(4, "delta", "Rust", 5),
            ],
        )
        .expect("seed");

        let page1 = list_repos(
            &conn,
            ListReposRequest {
                filters: Some(RepoFilters {
                    language: Some("Rust".into()),
                    ..Default::default()
                }),
                sort: Some(RepoSort::Stars),
                sort_desc: Some(true),
                limit: Some(2),
                offset: Some(0),
            },
        )
        .expect("p1");
        assert_eq!(page1.total, 3);
        assert_eq!(page1.items.len(), 2);
        assert_eq!(page1.items[0].full_name, "owner/gamma");
        assert_eq!(page1.items[1].full_name, "owner/alpha");

        let page2 = list_repos(
            &conn,
            ListReposRequest {
                filters: Some(RepoFilters {
                    language: Some("Rust".into()),
                    ..Default::default()
                }),
                sort: Some(RepoSort::Stars),
                sort_desc: Some(true),
                limit: Some(2),
                offset: Some(2),
            },
        )
        .expect("p2");
        assert_eq!(page2.total, 3);
        assert_eq!(page2.items.len(), 1);
        assert_eq!(page2.items[0].full_name, "owner/delta");
        assert_ne!(page1.items[0].id, page2.items[0].id);
    }

    fn sample_starred(id: i64, name: &str, starred_at: &str) -> StarredRepo {
        let mut repo = sample(id, name, "Rust", 1);
        repo.starred_at = starred_at.into();
        repo
    }

    #[test]
    fn pagination_tie_breaks_equal_starred_at_without_skip_or_dup() {
        let conn = test_conn();
        let ts = "2024-06-01T12:00:00Z";
        apply_full_diff(
            &conn,
            &[
                sample_starred(10, "zulu", ts),
                sample_starred(2, "alpha", ts),
                sample_starred(7, "mike", ts),
            ],
        )
        .expect("seed");

        let page1 = list_repos(
            &conn,
            ListReposRequest {
                filters: None,
                sort: Some(RepoSort::StarredAt),
                sort_desc: Some(true),
                limit: Some(2),
                offset: Some(0),
            },
        )
        .expect("p1");
        let page2 = list_repos(
            &conn,
            ListReposRequest {
                filters: None,
                sort: Some(RepoSort::StarredAt),
                sort_desc: Some(true),
                limit: Some(2),
                offset: Some(2),
            },
        )
        .expect("p2");

        assert_eq!(page1.total, 3);
        assert_eq!(page1.items.len(), 2);
        assert_eq!(page2.items.len(), 1);
        let ids1: Vec<i64> = page1.items.iter().map(|r| r.id).collect();
        let ids2: Vec<i64> = page2.items.iter().map(|r| r.id).collect();
        assert!(
            ids1.iter().all(|id| !ids2.contains(id)),
            "page boundary must not duplicate ids: {ids1:?} vs {ids2:?}"
        );
        // Equal starred_at → stable id ASC tie-break: 2, 7, then 10.
        assert_eq!(ids1, vec![2, 7]);
        assert_eq!(ids2, vec![10]);
    }
}
