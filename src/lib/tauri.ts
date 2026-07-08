import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, AuthStatus, UpdateSettingsRequest } from "@/types";

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
