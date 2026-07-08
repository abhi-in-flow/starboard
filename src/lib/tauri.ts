import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AuthStatus,
  CategoryNode,
  LibraryFacets,
  ListReposRequest,
  RepoDetail,
  RepoFilters,
  RepoListResult,
  SearchReposRequest,
  SyncResult,
  SyncStatus,
  UpdateSettingsRequest,
} from "@/types";

export function getSettings() {
  return invoke<AppSettings>("get_settings");
}

export function updateSettings(request: UpdateSettingsRequest) {
  return invoke<AppSettings>("update_settings", { request });
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
