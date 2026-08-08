import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AuthStatus,
  CategorizeStatus,
  CategoryNode,
  EmbedStatus,
  LibraryFacets,
  ListReposRequest,
  OllamaStatus,
  RepoDetail,
  RepoFilters,
  RepoListResult,
  SearchReposRequest,
  SyncResult,
  SyncStatus,
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
