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
    /// After sync + README drain, assign new/uncategorized repos via Ollama.
    /// Defaults on when the settings key is missing (new installs and upgrades).
    pub auto_categorize_after_sync: bool,
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
            auto_categorize_after_sync: true,
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
    pub auto_categorize_after_sync: Option<bool>,
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
    /// Last time a full starred-list walk (unstar-capable) completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_full_reconcile_at: Option<String>,
    /// True when the next incremental Sync will be promoted to a full reconcile.
    #[serde(default)]
    pub reconcile_due: bool,
    /// User-visible incremental/unstar policy (never claims incremental detects unstars).
    #[serde(default)]
    pub unstar_policy: String,
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
    /// Normalized fused relevance (0–100) for Semantic/Hybrid hits; absent for keyword/browse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relevance: Option<u8>,
    /// Why this repo is in the active review queue (preset reason copy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_reason: Option<String>,
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
    pub reviewed_at: Option<String>,
    pub snoozed_until: Option<String>,
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
    /// Local review queue preset. When set, reviewed/snoozed rows are excluded.
    #[serde(default)]
    pub review_preset: Option<ReviewPreset>,
}

/// First-class Library review queues. Local-only — no GitHub star writes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReviewPreset {
    Uncategorized,
    Archived,
    Inactive,
    Forgotten,
    Unstarred,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum RepoSort {
    #[default]
    StarredAt,
    Stars,
    PushedAt,
    Name,
    /// Oldest / most stale first (null `pushed_at`, then oldest push, then oldest star).
    Stale,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum SearchMode {
    #[default]
    Keyword,
    Semantic,
    Hybrid,
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
    /// Keyword | Semantic | Hybrid. Defaults to Keyword when omitted (browse-safe).
    pub mode: Option<SearchMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoListResult {
    pub items: Vec<RepoSummary>,
    pub total: i64,
    /// Mode actually used (may differ from requested on fallback).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_used: Option<SearchMode>,
    /// User-facing hint when Semantic/Hybrid degraded to Keyword.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Hybrid/semantic DB+RRF timing in ms, excluding the query-embedding HTTP call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_ms: Option<u64>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedProgress {
    pub kind: String,
    pub current: u32,
    pub total: u32,
    pub message: String,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmbedStatus {
    pub running: bool,
    pub total_repos: i64,
    pub embedded_repos: i64,
    pub stale_or_missing: i64,
    /// embedded_repos / total_repos, or 0 when empty.
    pub coverage: f64,
    pub last_error: Option<String>,
    pub model: String,
    pub dimension: i64,
    pub need_rebuild: bool,
}

/// Date-range filter for insights aggregations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InsightsDateRange {
    /// `all` | `1y` | `6m` | `custom`
    pub preset: Option<String>,
    /// Inclusive ISO-8601 start (used when preset is `custom`)
    pub start: Option<String>,
    /// Inclusive ISO-8601 end (used when preset is `custom`)
    pub end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InsightsRequest {
    pub range: Option<InsightsDateRange>,
    /// Minutes east of UTC (e.g. -480 for PST). From `-new Date().getTimezoneOffset()`.
    pub utc_offset_minutes: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightsMeta {
    pub total_stars: i64,
    pub span_days: i64,
    pub short_history: bool,
    pub range_start: Option<String>,
    pub range_end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineBucket {
    pub period: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StarringTimeline {
    pub weekly: Vec<TimelineBucket>,
    pub monthly: Vec<TimelineBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatmapCell {
    /// 0 = Monday … 6 = Sunday
    pub day_of_week: u8,
    /// 0–23 in the user's local timezone
    pub hour: u8,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharePoint {
    pub period: String,
    pub name: String,
    pub count: i64,
    pub share: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterestMetric {
    pub category: String,
    pub repo_count: i64,
    pub first_starred: Option<String>,
    pub last_starred: Option<String>,
    pub recent_velocity: f64,
    pub lifetime_velocity: f64,
    /// `rising` | `dormant` | `steady`
    pub badge: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunFacts {
    pub longest_streak_days: i64,
    pub biggest_day_count: i64,
    pub biggest_day_date: Option<String>,
    pub first_star_at: Option<String>,
    pub first_star_repo: Option<String>,
    pub oldest_repo_created_at: Option<String>,
    pub oldest_repo_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightsDashboard {
    pub meta: InsightsMeta,
    pub timeline: StarringTimeline,
    pub heatmap: Vec<HeatmapCell>,
    pub interest_drift: Vec<SharePoint>,
    pub language_trend: Vec<SharePoint>,
    pub interest_metrics: Vec<InterestMetric>,
    pub fun_facts: FunFacts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryExport {
    pub markdown: String,
    pub json: String,
}

/// Result of `PRAGMA integrity_check` + foreign-key check (Settings → Status).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityReport {
    pub ok: bool,
    pub integrity: String,
    pub foreign_key_violations: i64,
    pub checked_at: String,
}

/// Admin overview: embeddings + categorization + local data safety.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub embeddings: EmbeddingsPanel,
    pub categorization: CategorizationPanel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<IntegrityReport>,
    pub data: DataPanel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataPanel {
    pub db_path: String,
    pub last_backup_path: Option<String>,
    pub last_backup_at: Option<String>,
    pub last_backup_ok: Option<bool>,
    pub last_restore_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupValidation {
    pub ok: bool,
    pub path: String,
    pub schema_version: i64,
    pub repo_count: i64,
    pub category_count: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub path: String,
    pub created_at: String,
    pub schema_version: i64,
    pub repo_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub path: String,
    pub restored_at: String,
    pub schema_version: i64,
    pub migrated: bool,
    pub pre_restore_backup_path: String,
    pub repo_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingsPanel {
    pub model: String,
    pub dimension: i64,
    pub total_repos: i64,
    pub embedded_repos: i64,
    pub stale_repos: i64,
    pub missing_repos: i64,
    /// embedded_repos / total_repos, or 0 when empty.
    pub coverage: f64,
    pub last_embed_at: Option<String>,
    pub need_rebuild: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorizationPanel {
    pub total_repos: i64,
    pub categorized_repos: i64,
    pub uncategorized_repos: i64,
    pub llm_assignments: i64,
    pub manual_assignments: i64,
    pub auto_categorize_after_sync: bool,
    pub categories: Vec<CategoryNode>,
}

/// First-run checklist snapshot. Local DB + keyring truth only (no Ollama HTTP).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub github_connected: bool,
    pub github_username: Option<String>,
    pub last_synced_at: Option<String>,
    pub repo_count: i64,
    pub ollama_base_url: String,
    pub ollama_chat_model: String,
    /// Base URL set and chat model non-empty. Live reachability is `get_ollama_status`.
    pub ollama_configured: bool,
    pub category_count: i64,
    pub categorized_repos: i64,
    /// categorized_repos / repo_count, or 0 when empty.
    pub assignment_coverage: f64,
    pub embedded_repos: i64,
    /// embedded_repos / repo_count, or 0 when empty.
    pub embedding_coverage: f64,
    pub onboarding_completed: bool,
}

/// Queue sizes for review presets (active counts exclude unstarred except `unstarred`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCounts {
    pub uncategorized: i64,
    pub archived: i64,
    pub inactive: i64,
    pub forgotten: i64,
    pub unstarred: i64,
    /// Currently starred repos (`unstarred = 0`), including archived.
    pub active_stars: i64,
    pub oldest_starred_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRepoReviewRequest {
    pub repo_id: i64,
    pub reviewed: Option<bool>,
    /// `7 | 30 | 90 | 180` to snooze, `0` to clear. Ignored when `reviewed` is true.
    pub snooze_days: Option<i64>,
}
