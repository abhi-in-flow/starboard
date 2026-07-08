use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub ollama_base_url: String,
    pub ollama_chat_model: String,
    pub ollama_embed_model: String,
    pub github_username: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ollama_base_url: "http://127.0.0.1:11434".to_string(),
            ollama_chat_model: String::new(),
            ollama_embed_model: "nomic-embed-text".to_string(),
            github_username: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub connected: bool,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    pub ollama_base_url: Option<String>,
    pub ollama_chat_model: Option<String>,
    pub ollama_embed_model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubUser {
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    pub kind: String,
    pub current: u32,
    pub total: u32,
    pub message: String,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub running: bool,
    pub readme_running: bool,
    pub pending_readmes: i64,
    pub last_synced_at: Option<String>,
    pub last_result: Option<SyncResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub kind: String,
    pub repos_added: i64,
    pub repos_updated: i64,
    pub repos_removed: i64,
    pub readmes_fetched: i64,
    pub status: String,
    pub error: Option<String>,
}

/// Local representation of a starred repo ready for upsert.
#[derive(Debug, Clone)]
pub struct StarredRepo {
    pub id: i64,
    pub full_name: String,
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub topics: String,
    pub stars_count: Option<i64>,
    pub forks_count: Option<i64>,
    pub open_issues: Option<i64>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub html_url: String,
    pub archived: bool,
    pub fork: bool,
    pub repo_created_at: Option<String>,
    pub pushed_at: Option<String>,
    pub starred_at: String,
}

#[derive(Debug, Deserialize)]
pub struct StarredItem {
    pub starred_at: String,
    pub repo: GitHubRepo,
}

#[derive(Debug, Deserialize)]
pub struct GitHubRepo {
    pub id: i64,
    pub full_name: String,
    pub name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    pub stargazers_count: Option<i64>,
    pub forks_count: Option<i64>,
    pub open_issues_count: Option<i64>,
    pub license: Option<GitHubLicense>,
    pub homepage: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub fork: bool,
    pub created_at: Option<String>,
    pub pushed_at: Option<String>,
    pub owner: GitHubOwner,
}

#[derive(Debug, Deserialize)]
pub struct GitHubOwner {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubLicense {
    pub spdx_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubReadme {
    pub content: Option<String>,
    pub encoding: Option<String>,
}

impl From<StarredItem> for StarredRepo {
    fn from(item: StarredItem) -> Self {
        let topics = serde_json::to_string(&item.repo.topics).unwrap_or_else(|_| "[]".into());
        let license = item
            .repo
            .license
            .and_then(|l| l.spdx_id)
            .filter(|s| s != "NOASSERTION");

        Self {
            id: item.repo.id,
            full_name: item.repo.full_name,
            owner: item.repo.owner.login,
            name: item.repo.name,
            description: item.repo.description,
            language: item.repo.language,
            topics,
            stars_count: item.repo.stargazers_count,
            forks_count: item.repo.forks_count,
            open_issues: item.repo.open_issues_count,
            license,
            homepage: item.repo.homepage,
            html_url: item.repo.html_url,
            archived: item.repo.archived,
            fork: item.repo.fork,
            repo_created_at: item.repo.created_at,
            pushed_at: item.repo.pushed_at,
            starred_at: item.starred_at,
        }
    }
}
