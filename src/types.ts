/** Mirrored from src-tauri/src/models.rs — keep in sync. */

export type AppSettings = {
  ollamaBaseUrl: string;
  ollamaChatModel: string;
  ollamaEmbedModel: string;
  githubUsername: string | null;
};

export type AuthStatus = {
  connected: boolean;
  username: string | null;
};

export type UpdateSettingsRequest = {
  ollamaBaseUrl?: string;
  ollamaChatModel?: string;
  ollamaEmbedModel?: string;
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
