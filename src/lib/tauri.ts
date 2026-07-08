import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AuthStatus,
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
