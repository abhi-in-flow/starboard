use rusqlite::Connection;

use crate::error::AppResult;
use crate::models::SetupStatus;
use crate::services::settings::{self, KEY_GITHUB_USERNAME, KEY_LAST_SYNCED_AT};

/// Aggregate first-run checklist truth from SQLite + caller-supplied PAT presence.
/// `pat_present` comes from the keyring (same source as `get_auth_status`).
pub fn get_setup_status(conn: &Connection, pat_present: bool) -> AppResult<SetupStatus> {
    let app_settings = settings::get_settings(conn)?;
    let github_username = settings::get_value(conn, KEY_GITHUB_USERNAME)?;
    let github_connected = pat_present && github_username.is_some();
    let last_synced_at = settings::get_value(conn, KEY_LAST_SYNCED_AT)?;

    let repo_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM repos WHERE unstarred = 0",
        [],
        |row| row.get(0),
    )?;
    let category_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM categories", [], |row| row.get(0))?;
    let categorized_repos: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT r.id) FROM repos r
         JOIN repo_categories rc ON rc.repo_id = r.id
         WHERE r.unstarred = 0",
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

    let assignment_coverage = coverage(categorized_repos, repo_count);
    let embedding_coverage = coverage(embedded_repos, repo_count);
    let ollama_configured = !app_settings.ollama_base_url.trim().is_empty()
        && !app_settings.ollama_chat_model.trim().is_empty();

    Ok(SetupStatus {
        github_connected,
        github_username,
        last_synced_at,
        repo_count,
        ollama_base_url: app_settings.ollama_base_url,
        ollama_chat_model: app_settings.ollama_chat_model,
        ollama_configured,
        category_count,
        categorized_repos,
        assignment_coverage,
        embedded_repos,
        embedding_coverage,
        onboarding_completed: settings::get_onboarding_completed(conn)?,
    })
}

fn coverage(part: i64, total: i64) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::UpdateSettingsRequest;
    use crate::services::embed::{content_hash, upsert_embedding};
    use crate::services::store::open_and_migrate;
    use rusqlite::params;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_conn() -> Connection {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("starboard_setup_{nanos}.db"));
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
    fn empty_db_is_incomplete() {
        let conn = test_conn();
        let status = get_setup_status(&conn, false).expect("status");
        assert!(!status.github_connected);
        assert!(status.github_username.is_none());
        assert!(status.last_synced_at.is_none());
        assert_eq!(status.repo_count, 0);
        assert!(!status.ollama_configured);
        assert_eq!(status.category_count, 0);
        assert_eq!(status.categorized_repos, 0);
        assert_eq!(status.assignment_coverage, 0.0);
        assert_eq!(status.embedded_repos, 0);
        assert_eq!(status.embedding_coverage, 0.0);
        assert!(!status.onboarding_completed);
    }

    #[test]
    fn auth_only_is_connected_but_unsynced() {
        let conn = test_conn();
        settings::set_github_username(&conn, "octocat").expect("username");
        let status = get_setup_status(&conn, true).expect("status");
        assert!(status.github_connected);
        assert_eq!(status.github_username.as_deref(), Some("octocat"));
        assert!(status.last_synced_at.is_none());
        assert_eq!(status.repo_count, 0);
        assert_eq!(status.category_count, 0);
        assert_eq!(status.embedded_repos, 0);
        assert!(!status.onboarding_completed);
    }

    #[test]
    fn username_without_pat_is_not_connected() {
        let conn = test_conn();
        settings::set_github_username(&conn, "octocat").expect("username");
        let status = get_setup_status(&conn, false).expect("status");
        assert!(!status.github_connected);
        assert_eq!(status.github_username.as_deref(), Some("octocat"));
    }

    #[test]
    fn synced_without_ai_is_usable() {
        let conn = test_conn();
        settings::set_github_username(&conn, "octocat").expect("username");
        settings::set_value(&conn, KEY_LAST_SYNCED_AT, "2024-06-01T00:00:00Z").expect("sync");
        insert_repo(&conn, 1, "alpha", false);
        insert_repo(&conn, 2, "beta", false);
        insert_repo(&conn, 3, "gone", true);

        let status = get_setup_status(&conn, true).expect("status");
        assert!(status.github_connected);
        assert_eq!(
            status.last_synced_at.as_deref(),
            Some("2024-06-01T00:00:00Z")
        );
        assert_eq!(status.repo_count, 2);
        assert!(!status.ollama_configured);
        assert_eq!(status.category_count, 0);
        assert_eq!(status.categorized_repos, 0);
        assert_eq!(status.assignment_coverage, 0.0);
        assert_eq!(status.embedded_repos, 0);
        assert_eq!(status.embedding_coverage, 0.0);
        assert!(!status.onboarding_completed);
    }

    #[test]
    fn fully_complete_aggregates_ai_coverage() {
        let conn = test_conn();
        settings::set_github_username(&conn, "octocat").expect("username");
        settings::set_value(&conn, KEY_LAST_SYNCED_AT, "2024-06-01T00:00:00Z").expect("sync");
        settings::update_settings(
            &conn,
            UpdateSettingsRequest {
                ollama_base_url: Some("http://192.168.1.10:11434".into()),
                ollama_chat_model: Some("qwen3:14b".into()),
                ollama_embed_model: None,
                embed_dimension: None,
            },
        )
        .expect("ollama settings");
        settings::set_onboarding_completed(&conn, true).expect("dismiss");

        insert_repo(&conn, 1, "alpha", false);
        insert_repo(&conn, 2, "beta", false);

        conn.execute(
            "INSERT INTO categories (id, name, parent_id) VALUES
             (1, 'AI', NULL),
             (2, 'Agents', 1)",
            [],
        )
        .expect("cats");
        conn.execute(
            "INSERT INTO repo_categories (repo_id, category_id, source, confidence) VALUES
             (1, 2, 'llm', 0.9),
             (2, 1, 'manual', 1.0)",
            [],
        )
        .expect("assign");

        let dim = 768_i64;
        let emb = vec![0.1_f32; dim as usize];
        upsert_embedding(
            &conn,
            1,
            &emb,
            &content_hash("owner/alpha\ndesc\nTopics: []\n"),
            "nomic-embed-text",
            dim,
        )
        .expect("embed 1");
        upsert_embedding(
            &conn,
            2,
            &emb,
            &content_hash("owner/beta\ndesc\nTopics: []\n"),
            "nomic-embed-text",
            dim,
        )
        .expect("embed 2");

        let status = get_setup_status(&conn, true).expect("status");
        assert!(status.github_connected);
        assert_eq!(status.repo_count, 2);
        assert!(status.ollama_configured);
        assert_eq!(status.ollama_chat_model, "qwen3:14b");
        assert_eq!(status.category_count, 2);
        assert_eq!(status.categorized_repos, 2);
        assert!((status.assignment_coverage - 1.0).abs() < 1e-9);
        assert_eq!(status.embedded_repos, 2);
        assert!((status.embedding_coverage - 1.0).abs() < 1e-9);
        assert!(status.onboarding_completed);
    }

    #[test]
    fn optional_ai_steps_do_not_block_dismissal() {
        let conn = test_conn();
        let before = get_setup_status(&conn, false).expect("before");
        assert!(!before.onboarding_completed);
        assert_eq!(before.category_count, 0);
        assert_eq!(before.embedded_repos, 0);

        settings::set_onboarding_completed(&conn, true).expect("dismiss");
        let after = get_setup_status(&conn, false).expect("after");
        assert!(after.onboarding_completed);
        assert_eq!(after.category_count, 0);
        assert_eq!(after.embedded_repos, 0);

        settings::set_onboarding_completed(&conn, false).expect("reopen");
        let again = get_setup_status(&conn, false).expect("reopen");
        assert!(!again.onboarding_completed);
    }
}
