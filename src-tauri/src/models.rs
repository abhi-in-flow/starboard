use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub ollama_base_url: String,
    pub ollama_chat_model: String,
    pub ollama_embed_model: String,
    pub embed_dimension: i64,
    pub github_username: Option<String>,
    /// True when settings `embed_dimension` differs from the live vec0 table.
    pub embeddings_need_rebuild: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ollama_base_url: "http://127.0.0.1:11434".to_string(),
            ollama_chat_model: String::new(),
            ollama_embed_model: "nomic-embed-text".to_string(),
            embed_dimension: 768,
            github_username: None,
            embeddings_need_rebuild: false,
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
    pub embed_dimension: Option<i64>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    pub id: i64,
    pub full_name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stars_count: Option<i64>,
    pub starred_at: String,
    pub pushed_at: Option<String>,
    pub archived: bool,
    pub unstarred: bool,
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoDetail {
    pub id: i64,
    pub full_name: String,
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub topics: Vec<String>,
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
    pub readme_excerpt: Option<String>,
    pub fetched_at: String,
    pub unstarred: bool,
    pub category_names: Vec<String>,
    pub category_id: Option<i64>,
    pub category_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepoFilters {
    pub language: Option<String>,
    pub topic: Option<String>,
    pub category_id: Option<i64>,
    pub hide_unstarred: Option<bool>,
    pub hide_archived: Option<bool>,
    pub archived_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum RepoSort {
    #[default]
    StarredAt,
    Stars,
    PushedAt,
    Name,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListReposRequest {
    pub filters: Option<RepoFilters>,
    pub sort: Option<RepoSort>,
    pub sort_desc: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchReposRequest {
    pub query: String,
    pub filters: Option<RepoFilters>,
    pub sort: Option<RepoSort>,
    pub sort_desc: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoListResult {
    pub items: Vec<RepoSummary>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacetCount {
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFacets {
    pub languages: Vec<FacetCount>,
    pub topics: Vec<FacetCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryNode {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub count: i64,
    pub children: Vec<CategoryNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaStatus {
    pub available: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxonomyCategoryDraft {
    pub name: String,
    pub subcategories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TaxonomyDraft {
    pub categories: Vec<TaxonomyCategoryDraft>,
}

/// Editable taxonomy node with stable DB ids (None = new row).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxonomyNodeEdit {
    pub id: Option<i64>,
    pub name: String,
    #[serde(default)]
    pub subcategories: Vec<TaxonomyNodeEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TaxonomyEdit {
    pub categories: Vec<TaxonomyNodeEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorizeProgress {
    pub kind: String,
    pub current: u32,
    pub total: u32,
    pub message: String,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CategorizeStatus {
    pub running: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignRepoCategoryRequest {
    pub repo_id: i64,
    pub category_id: i64,
}
