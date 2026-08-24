use rusqlite::{Connection, OptionalExtension};

use crate::error::AppResult;
use crate::models::{CategorizationPanel, EmbeddingsPanel, SystemStatus};
use crate::services::categories;
use crate::services::embed;
use crate::services::settings;

/// Aggregate embeddings + categorization admin stats for the Settings Status tab.
pub fn get_system_status(conn: &Connection) -> AppResult<SystemStatus> {
    Ok(SystemStatus {
        embeddings: embeddings_panel(conn)?,
        categorization: categorization_panel(conn)?,
    })
}

fn embeddings_panel(conn: &Connection) -> AppResult<EmbeddingsPanel> {
    let app_settings = settings::get_settings(conn)?;
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
    let stale_or_missing = embed::list_stale_or_missing(conn)?.len() as i64;
    let stale_repos = (stale_or_missing - missing_repos).max(0);
    let coverage = if total_repos == 0 {
        0.0
    } else {
        embedded_repos as f64 / total_repos as f64
    };
    let last_embed_at: Option<String> = conn
        .query_row(
            "SELECT finished_at FROM sync_log
             WHERE kind = 'embed' AND finished_at IS NOT NULL
             ORDER BY id DESC
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;

    Ok(EmbeddingsPanel {
        model: app_settings.ollama_embed_model,
        dimension: app_settings.embed_dimension,
        total_repos,
        embedded_repos,
        stale_repos,
        missing_repos,
        coverage,
        last_embed_at,
        need_rebuild: app_settings.embeddings_need_rebuild,
    })
}

fn categorization_panel(conn: &Connection) -> AppResult<CategorizationPanel> {
    let total_repos: i64 = conn.query_row(
        "SELECT COUNT(*) FROM repos WHERE unstarred = 0",
        [],
        |row| row.get(0),
    )?;
    let categorized_repos: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT r.id) FROM repos r
         JOIN repo_categories rc ON rc.repo_id = r.id
         WHERE r.unstarred = 0",
        [],
        |row| row.get(0),
    )?;
    let uncategorized_repos = (total_repos - categorized_repos).max(0);
    let llm_assignments: i64 = conn.query_row(
        "SELECT COUNT(*) FROM repo_categories rc
         JOIN repos r ON r.id = rc.repo_id
         WHERE r.unstarred = 0 AND rc.source = 'llm'",
        [],
        |row| row.get(0),
    )?;
    let manual_assignments: i64 = conn.query_row(
        "SELECT COUNT(*) FROM repo_categories rc
         JOIN repos r ON r.id = rc.repo_id
         WHERE r.unstarred = 0 AND rc.source = 'manual'",
        [],
        |row| row.get(0),
    )?;
    let categories = categories::list_category_tree(conn)?;

    Ok(CategorizationPanel {
        total_repos,
        categorized_repos,
        uncategorized_repos,
        llm_assignments,
        manual_assignments,
        categories,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::embed::{content_hash, upsert_embedding};
    use crate::services::store::open_and_migrate;
    use rusqlite::params;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_conn() -> Connection {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("starboard_status_{nanos}.db"));
        open_and_migrate(&path).expect("migrate")
    }

    fn insert_repo(conn: &Connection, id: i64, name: &str, unstarred: bool) {
        conn.execute(
            "INSERT INTO repos (
                id, full_name, owner, name, description, language, topics,
                stars_count, forks_count, open_issues, license, homepage, html_url,
                archived, fork, repo_created_at, pushed_at, starred_at,
                readme_excerpt, fetched_at, unstarred
             ) VALUES (
                ?1, ?2, 'owner', ?3, 'desc', 'Rust', '[]',
                10, 1, 0, 'MIT', NULL, ?4,
                0, 0, '2020-01-01T00:00:00Z', '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z',
                NULL, '2024-01-01T00:00:00Z', ?5
             )",
            params![
                id,
                format!("owner/{name}"),
                name,
                format!("https://github.com/owner/{name}"),
                if unstarred { 1 } else { 0 },
            ],
        )
        .expect("insert repo");
    }

    #[test]
    fn empty_db_is_safe() {
        let conn = test_conn();
        let status = get_system_status(&conn).expect("status");
        assert_eq!(status.embeddings.total_repos, 0);
        assert_eq!(status.embeddings.embedded_repos, 0);
        assert_eq!(status.embeddings.stale_repos, 0);
        assert_eq!(status.embeddings.missing_repos, 0);
        assert_eq!(status.embeddings.coverage, 0.0);
        assert!(status.embeddings.last_embed_at.is_none());
        assert_eq!(status.categorization.total_repos, 0);
        assert_eq!(status.categorization.categorized_repos, 0);
        assert_eq!(status.categorization.uncategorized_repos, 0);
        assert_eq!(status.categorization.llm_assignments, 0);
        assert_eq!(status.categorization.manual_assignments, 0);
        assert!(status.categorization.categories.is_empty());
    }

    #[test]
    fn aggregates_embeddings_and_categorization() {
        let conn = test_conn();
        insert_repo(&conn, 1, "a", false);
        insert_repo(&conn, 2, "b", false);
        insert_repo(&conn, 3, "c", false);
        insert_repo(&conn, 4, "gone", true); // unstarred — ignored

        let dim = 768_i64;
        let emb = vec![0.1_f32; dim as usize];
        // Repo 1: fresh embedding (hash must match embed::build_document)
        upsert_embedding(
            &conn,
            1,
            &emb,
            &content_hash("owner/a\ndesc\nTopics: []\n"),
            "nomic-embed-text",
            dim,
        )
        .expect("embed 1");
        // Repo 2: stale embedding (wrong hash)
        upsert_embedding(&conn, 2, &emb, "stale-hash", "nomic-embed-text", dim).expect("embed 2");
        // Repo 3: missing

        conn.execute(
            "INSERT INTO sync_log (started_at, finished_at, kind, status)
             VALUES ('2024-06-01T00:00:00Z', '2024-06-01T00:05:00Z', 'embed', 'ok')",
            [],
        )
        .expect("sync_log");

        conn.execute(
            "INSERT INTO categories (id, name, parent_id) VALUES
             (1, 'AI', NULL),
             (2, 'Agents', 1),
             (3, 'DevTools', NULL)",
            [],
        )
        .expect("cats");
        conn.execute(
            "INSERT INTO repo_categories (repo_id, category_id, source, confidence) VALUES
             (1, 2, 'llm', 0.9),
             (2, 3, 'manual', 1.0)",
            [],
        )
        .expect("assign");

        let status = get_system_status(&conn).expect("status");

        assert_eq!(status.embeddings.total_repos, 3);
        assert_eq!(status.embeddings.embedded_repos, 2);
        assert_eq!(status.embeddings.missing_repos, 1);
        assert_eq!(status.embeddings.stale_repos, 1);
        assert!((status.embeddings.coverage - (2.0 / 3.0)).abs() < 1e-9);
        assert_eq!(
            status.embeddings.last_embed_at.as_deref(),
            Some("2024-06-01T00:05:00Z")
        );

        assert_eq!(status.categorization.total_repos, 3);
        assert_eq!(status.categorization.categorized_repos, 2);
        assert_eq!(status.categorization.uncategorized_repos, 1);
        assert_eq!(status.categorization.llm_assignments, 1);
        assert_eq!(status.categorization.manual_assignments, 1);

        let ai = status
            .categorization
            .categories
            .iter()
            .find(|c| c.name == "AI")
            .expect("AI");
        assert_eq!(ai.children.len(), 1);
        assert_eq!(ai.children[0].name, "Agents");
        assert_eq!(ai.children[0].count, 1);
        let tools = status
            .categorization
            .categories
            .iter()
            .find(|c| c.name == "DevTools")
            .expect("DevTools");
        assert_eq!(tools.count, 1);
    }
}
