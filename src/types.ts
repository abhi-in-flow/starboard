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
