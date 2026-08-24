/** Mirrored from src-tauri/src/models.rs — keep in sync. */

export type AppSettings = {
  ollamaBaseUrl: string;
  ollamaChatModel: string;
  ollamaEmbedModel: string;
  embedDimension: number;
  githubUsername: string | null;
  embeddingsNeedRebuild: boolean;
  /** Defaults on when unset — assign new/uncategorized repos after sync. */
  autoCategorizeAfterSync: boolean;
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
  autoCategorizeAfterSync?: boolean;
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
  lastFullReconcileAt?: string | null;
  reconcileDue?: boolean;
  unstarPolicy?: string;
};

export type IntegrityReport = {
  ok: boolean;
  integrity: string;
  foreignKeyViolations: number;
  checkedAt: string;
};

export type RepoSort = "starredAt" | "stars" | "pushedAt" | "name" | "stale";

export type ReviewPreset =
  | "uncategorized"
  | "archived"
  | "inactive"
  | "forgotten"
  | "unstarred";

export type RepoFilters = {
  language?: string | null;
  topic?: string | null;
  categoryId?: number | null;
  hideUnstarred?: boolean;
  hideArchived?: boolean;
  archivedOnly?: boolean;
  reviewPreset?: ReviewPreset | null;
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
  /** Normalized fused relevance 0–100 for Semantic/Hybrid; absent for keyword/browse. */
  relevance?: number | null;
  /** Why this repo is in the active review queue. */
  reviewReason?: string | null;
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
  reviewedAt: string | null;
  snoozedUntil: string | null;
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

export type InsightsDateRange = {
  preset?: string | null;
  start?: string | null;
  end?: string | null;
};

export type InsightsRequest = {
  range?: InsightsDateRange | null;
  /** Minutes east of UTC; pass `-new Date().getTimezoneOffset()`. */
  utcOffsetMinutes?: number | null;
};

export type InsightsMeta = {
  totalStars: number;
  spanDays: number;
  shortHistory: boolean;
  rangeStart: string | null;
  rangeEnd: string | null;
};

export type TimelineBucket = {
  period: string;
  count: number;
};

export type StarringTimeline = {
  weekly: TimelineBucket[];
  monthly: TimelineBucket[];
};

export type HeatmapCell = {
  /** 0 = Monday … 6 = Sunday */
  dayOfWeek: number;
  hour: number;
  count: number;
};

export type SharePoint = {
  period: string;
  name: string;
  count: number;
  share: number;
};

export type InterestMetric = {
  category: string;
  repoCount: number;
  firstStarred: string | null;
  lastStarred: string | null;
  recentVelocity: number;
  lifetimeVelocity: number;
  badge: "rising" | "dormant" | "steady" | string;
};

export type FunFacts = {
  longestStreakDays: number;
  biggestDayCount: number;
  biggestDayDate: string | null;
  firstStarAt: string | null;
  firstStarRepo: string | null;
  oldestRepoCreatedAt: string | null;
  oldestRepoName: string | null;
};

export type InsightsDashboard = {
  meta: InsightsMeta;
  timeline: StarringTimeline;
  heatmap: HeatmapCell[];
  interestDrift: SharePoint[];
  languageTrend: SharePoint[];
  interestMetrics: InterestMetric[];
  funFacts: FunFacts;
};

export type LibraryExport = {
  markdown: string;
  json: string;
};

export type EmbeddingsPanel = {
  model: string;
  dimension: number;
  totalRepos: number;
  embeddedRepos: number;
  staleRepos: number;
  missingRepos: number;
  coverage: number;
  lastEmbedAt: string | null;
  needRebuild: boolean;
};

export type CategorizationPanel = {
  totalRepos: number;
  categorizedRepos: number;
  uncategorizedRepos: number;
  llmAssignments: number;
  manualAssignments: number;
  autoCategorizeAfterSync: boolean;
  categories: CategoryNode[];
};

export type DataPanel = {
  dbPath: string;
  lastBackupPath: string | null;
  lastBackupAt: string | null;
  lastBackupOk: boolean | null;
  lastRestoreAt: string | null;
};

export type IntegrityCheckResult = {
  ok: boolean;
  message: string;
};

export type BackupValidation = {
  ok: boolean;
  path: string;
  schemaVersion: number;
  repoCount: number;
  categoryCount: number;
  message: string;
};

export type BackupResult = {
  path: string;
  createdAt: string;
  schemaVersion: number;
  repoCount: number;
};

export type RestoreResult = {
  path: string;
  restoredAt: string;
  schemaVersion: number;
  migrated: boolean;
  preRestoreBackupPath: string;
  repoCount: number;
};

export type SystemStatus = {
  embeddings: EmbeddingsPanel;
  categorization: CategorizationPanel;
  integrity?: IntegrityReport | null;
  data: DataPanel;
};

/** Mirrored from SetupStatus — first-run checklist snapshot (no Ollama HTTP). */
export type SetupStatus = {
  githubConnected: boolean;
  githubUsername: string | null;
  lastSyncedAt: string | null;
  repoCount: number;
  ollamaBaseUrl: string;
  ollamaChatModel: string;
  ollamaConfigured: boolean;
  categoryCount: number;
  categorizedRepos: number;
  assignmentCoverage: number;
  embeddedRepos: number;
  embeddingCoverage: number;
  onboardingCompleted: boolean;
};

export type ReviewCounts = {
  uncategorized: number;
  archived: number;
  inactive: number;
  forgotten: number;
  unstarred: number;
  activeStars: number;
  oldestStarredAt: string | null;
};

export type SetRepoReviewRequest = {
  repoId: number;
  reviewed?: boolean | null;
  snoozeDays?: number | null;
};
