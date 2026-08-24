import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AuthStatus,
  BackupResult,
  BackupValidation,
  CategorizeStatus,
  CategoryNode,
  EmbedStatus,
  InsightsDashboard,
  InsightsDateRange,
  InsightsRequest,
  IntegrityReport,  LibraryExport,
  LibraryFacets,
  ListReposRequest,
  OllamaStatus,
  RepoDetail,
  RepoFilters,
  RepoListResult,
  RestoreResult,
  ReviewCounts,  SearchReposRequest,
  SetRepoReviewRequest,
  SetupStatus,
  SharePoint,
  SyncResult,
  SyncStatus,
  SystemStatus,
  TaxonomyDraft,
  TaxonomyEdit,
  UpdateSettingsRequest,
} from "@/types";

export function getSettings() {
  return invoke<AppSettings>("get_settings");
}

export function updateSettings(request: UpdateSettingsRequest) {
  return invoke<AppSettings>("update_settings", { request });
}

export function rebuildEmbeddingsTable(dimension?: number) {
  return invoke<AppSettings>("rebuild_embeddings_table", {
    dimension: dimension ?? null,
  });
}

export function getAuthStatus() {
  return invoke<AuthStatus>("get_auth_status");
}

export function connectGithub(pat: string) {
  return invoke<AuthStatus>("connect_github", { pat });
}

export function disconnectGithub() {
  return invoke<AuthStatus>("disconnect_github");
}

export function startSync(full = false) {
  return invoke<SyncResult>("start_sync", { full });
}

export function resumeReadmeQueue() {
  return invoke<void>("resume_readme_queue");
}

export function getSyncStatus() {
  return invoke<SyncStatus>("get_sync_status");
}

export function cancelSync() {
  return invoke<void>("cancel_sync");
}

export function cancelReadmeQueue() {
  return invoke<void>("cancel_readme_queue");
}

export function listRepos(request: ListReposRequest = {}) {
  return invoke<RepoListResult>("list_repos", { request });
}

export function searchRepos(request: SearchReposRequest) {
  return invoke<RepoListResult>("search_repos", { request });
}

export function getRepo(id: number) {
  return invoke<RepoDetail>("get_repo", { id });
}

export function getLibraryFacets(filters?: RepoFilters) {
  return invoke<LibraryFacets>("get_library_facets", {
    filters: filters ?? null,
  });
}

export function listCategories() {
  return invoke<CategoryNode[]>("list_categories");
}

export function getOllamaStatus() {
  return invoke<OllamaStatus>("get_ollama_status");
}

export function generateTaxonomy() {
  return invoke<TaxonomyDraft>("generate_taxonomy");
}

export function getTaxonomyEdit() {
  return invoke<TaxonomyEdit>("get_taxonomy_edit");
}

export function updateTaxonomy(edit: TaxonomyEdit) {
  return invoke<void>("update_taxonomy", { edit });
}

export function commitTaxonomy(draft: TaxonomyDraft, force = false) {
  return invoke<void>("commit_taxonomy", { draft, force });
}

export function startAssignment() {
  return invoke<void>("start_assignment");
}

export function cancelAssignment() {
  return invoke<void>("cancel_assignment");
}

export function getCategorizeStatus() {
  return invoke<CategorizeStatus>("get_categorize_status");
}

export function setRepoCategory(repoId: number, categoryId: number) {
  return invoke<void>("set_repo_category", {
    request: { repoId, categoryId },
  });
}

export function recategorizeRepo(repoId: number) {
  return invoke<void>("recategorize_repo", { repoId });
}

export function getEmbedStatus() {
  return invoke<EmbedStatus>("get_embed_status");
}

export function startEmbedding() {
  return invoke<void>("start_embedding");
}

export function cancelEmbedding() {
  return invoke<void>("cancel_embedding");
}

export function getInsights(request: InsightsRequest = {}) {
  return invoke<InsightsDashboard>("get_insights", { request });
}

export function getInterestDriftDrilldown(
  category: string,
  range?: InsightsDateRange | null,
  utcOffsetMinutes?: number | null,
) {
  return invoke<SharePoint[]>("get_interest_drift_drilldown", {
    range: range ?? null,
    category,
    utcOffsetMinutes: utcOffsetMinutes ?? null,
  });
}

export function getLibraryExport() {
  return invoke<LibraryExport>("get_library_export");
}

export function writeLibraryExport(path: string, format: "markdown" | "json") {
  return invoke<void>("write_library_export", { path, format });
}

export function getSystemStatus() {
  return invoke<SystemStatus>("get_system_status");
}

export function checkDbIntegrity() {
  return invoke<IntegrityReport>("check_db_integrity");
}

export function getSetupStatus() {
  return invoke<SetupStatus>("get_setup_status");
}

export function setOnboardingCompleted(completed: boolean) {
  return invoke<SetupStatus>("set_onboarding_completed", { completed });
}

export function getReviewCounts() {
  return invoke<ReviewCounts>("get_review_counts");
}

export function setRepoReview(request: SetRepoReviewRequest) {
  return invoke<void>("set_repo_review", { request });
}

export function createBackup(path: string) {
  return invoke<BackupResult>("create_backup", { path });
}

export function validateBackup(path: string) {
  return invoke<BackupValidation>("validate_backup", { path });
}

export function restoreBackup(path: string) {
  return invoke<RestoreResult>("restore_backup", { path });
}
