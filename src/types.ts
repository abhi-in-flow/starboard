/** Mirrored from src-tauri/src/models.rs — keep in sync. */

export type AppSettings = {
  ollamaBaseUrl: string;
  ollamaChatModel: string;
  ollamaEmbedModel: string;
  embedDimension: number;
  githubUsername: string | null;
  embeddingsNeedRebuild: boolean;
};

export type AuthStatus = {
  connected: boolean;
  username: string | null;
};

export type UpdateSettingsRequest = {
  ollamaBaseUrl?: string;
  ollamaChatModel?: string;
  ollamaEmbedModel?: string;
  embedDimension?: number;
};

export type AppError = {
  code: string;
  message: string;
};

export type SyncProgress = {
  kind: string;
  current: number;
  total: number;
  message: string;
  error?: string | null;
};

export type SyncResult = {
  kind: string;
  reposAdded: number;
  reposUpdated: number;
  reposRemoved: number;
  readmesFetched: number;
  status: string;
  error: string | null;
};

export type SyncStatus = {
  running: boolean;
  readmeRunning: boolean;
  pendingReadmes: number;
  lastSyncedAt: string | null;
  lastResult: SyncResult | null;
};

export type RepoSort = "starredAt" | "stars" | "pushedAt" | "name";

export type RepoFilters = {
  language?: string | null;
  topic?: string | null;
  categoryId?: number | null;
  hideUnstarred?: boolean;
  hideArchived?: boolean;
  archivedOnly?: boolean;
};

export type RepoSummary = {
  id: number;
  fullName: string;
  description: string | null;
  language: string | null;
  starsCount: number | null;
  starredAt: string;
  pushedAt: string | null;
  archived: boolean;
  unstarred: boolean;
  topics: string[];
};

export type RepoDetail = {
  id: number;
  fullName: string;
  owner: string;
  name: string;
  description: string | null;
  language: string | null;
  topics: string[];
  starsCount: number | null;
  forksCount: number | null;
  openIssues: number | null;
  license: string | null;
  homepage: string | null;
  htmlUrl: string;
  archived: boolean;
  fork: boolean;
  repoCreatedAt: string | null;
  pushedAt: string | null;
  starredAt: string;
  readmeExcerpt: string | null;
  fetchedAt: string;
  unstarred: boolean;
  categoryNames: string[];
  categoryId: number | null;
  categorySource: string | null;
};

export type RepoListResult = {
  items: RepoSummary[];
  total: number;
};

export type FacetCount = {
  name: string;
  count: number;
};

export type LibraryFacets = {
  languages: FacetCount[];
  topics: FacetCount[];
};

export type CategoryNode = {
  id: number;
  name: string;
  parentId: number | null;
  count: number;
  children: CategoryNode[];
};

export type ListReposRequest = {
  filters?: RepoFilters;
  sort?: RepoSort;
  sortDesc?: boolean;
  limit?: number;
  offset?: number;
};

export type SearchMode = "keyword" | "semantic" | "hybrid";

export type SearchReposRequest = {
  query: string;
  filters?: RepoFilters;
  sort?: RepoSort;
  sortDesc?: boolean;
  limit?: number;
  offset?: number;
  mode?: SearchMode;
};

export type RepoListResult = {
  items: RepoSummary[];
  total: number;
  modeUsed?: SearchMode;
  hint?: string | null;
  searchMs?: number | null;
};

export type OllamaStatus = {
  available: boolean;
  message: string;
};

export type TaxonomyCategoryDraft = {
  name: string;
  subcategories: string[];
};

export type TaxonomyDraft = {
  categories: TaxonomyCategoryDraft[];
};

export type TaxonomyNodeEdit = {
  id: number | null;
  name: string;
  subcategories: TaxonomyNodeEdit[];
};

export type TaxonomyEdit = {
  categories: TaxonomyNodeEdit[];
};

export type CategorizeProgress = {
  kind: string;
  current: number;
  total: number;
  message: string;
  error?: string | null;
};

export type CategorizeStatus = {
  running: boolean;
  lastError: string | null;
};

export type AssignRepoCategoryRequest = {
  repoId: number;
  categoryId: number;
};

export type EmbedProgress = {
  kind: string;
  current: number;
  total: number;
  message: string;
  error?: string | null;
};

export type EmbedStatus = {
  running: boolean;
  totalRepos: number;
  embeddedRepos: number;
  staleOrMissing: number;
  coverage: number;
  lastError: string | null;
  model: string;
  dimension: number;
  needRebuild: boolean;
};
