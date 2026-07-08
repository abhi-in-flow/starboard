# Task: Phase 0 — Scaffold & Foundations

Read `CLAUDE.md` and `docs/HLD.md` §2–§3 first.

## Goal
A launching Tauri app with the database, settings, and PAT flow in place. No GitHub sync yet.

## Build

1. Scaffold with `create-tauri-app` (Tauri 2.x, React + TS + Vite). Add Tailwind + shadcn/ui. Set app identifier `com.brocode.starboard`, window title "Starboard".
2. SQLite in the Tauri app-data dir. Migration runner (`rusqlite_migration`) with **migration 001**: the full schema from HLD §3 — `repos`, `categories`, `repo_categories`, `sync_log`, `settings`, `repos_fts` + its sync triggers. Do NOT include the vec0 table (Phase 4).
3. Settings service: get/set over the `settings` table. Keys: `ollama_base_url` (default `http://127.0.0.1:11434`), `ollama_chat_model`, `ollama_embed_model` (default `nomic-embed-text`), `github_username`. Minimal settings screen in the UI.
4. PAT onboarding: input screen → validate with `GET https://api.github.com/user` (Bearer auth) → on success store PAT in OS keyring (`keyring` crate, service `starboard`, account `github_pat`) and persist `github_username` from the response. On failure show the GitHub error message. The PAT is never displayed back; show "Connected as <username>" with a "Replace token" action.
5. Skeleton app shell: sidebar (Library / Insights placeholder / Settings), empty library view.
6. Wire the check commands from CLAUDE.md; add the log-scrubber test (assert a formatted log line containing an Authorization header is redacted).

## Out of scope
Any GitHub starred-repo fetching, categorization, search UI.

## Acceptance criteria
- App launches on Windows 11; fresh run creates the DB with all tables and triggers (verify with `sqlite3 .schema`).
- PAT survives app restart via keyring; invalid PAT shows a clear inline error.
- All checks clean per CLAUDE.md DoD.
