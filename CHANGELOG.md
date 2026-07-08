# Changelog

## 0.2.0 — Phase 1

- Add GitHub starred-repo sync with `star+json` Accept header, pagination, ETag 304 fast path, and rate-limit backoff.
- Upsert repos with unstar soft-delete + `sync_log`; concurrent background README queue (~6 in-flight / ~6 req/s) with soft-fail and Resume READMEs.
- Emit `sync://progress` (incl. errors); header Sync / Full sync / Resume READMEs with progress bar and last-synced time.

## 0.1.0 — Phase 0

- Scaffold Tauri 2 + React 18 + TypeScript + Vite with Bun, Tailwind v4, shadcn/ui, Zustand, TanStack Query, and Biome.
- Add SQLite migration 001 (full schema minus vec0), settings service, and PAT onboarding via OS keyring.
- Ship app shell (Library / Insights / Settings) with log-scrubber test and clean clippy/test/lint/tsc checks.
