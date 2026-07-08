# Task: Phase 1 — Sync Engine

**Status:** Complete (as-built notes below)  
Read `CLAUDE.md` and `docs/architecture/starboard-hld-and-plan.md` §4.1 first. Depends on Phase 0.

## Goal
Full + incremental sync of the user's starred repos into SQLite, with `starred_at`, README excerpts, and live progress in the UI.

## Build

1. `services/github.rs` — REST client:
   - `GET /user/starred?per_page=100` with headers: `Authorization: Bearer <PAT>`, `Accept: application/vnd.github.star+json`, `X-GitHub-Api-Version: 2022-11-28`. The star+json media type returns `{ starred_at, repo }` objects — this is mandatory (CLAUDE.md rule 5). Add a wiremock test asserting the Accept header.
   - Pagination via the `Link: rel="next"` header.
   - ETag of page 1 cached in `settings`; incremental sync sends `If-None-Match`, treats `304` as "no changes".
   - Rate-limit guard: read `x-ratelimit-remaining` / `x-ratelimit-reset`; when remaining < 50, sleep until reset + jitter. On `403`/`429` with `retry-after`, honor it with exponential backoff (max 3 retries).
2. Sync service:
   - **Full sync:** fetch all pages → upsert into `repos` → set difference vs local non-unstarred rows → flag missing ones `unstarred = 1`. Record a `sync_log` row (kind `full`).
   - **Incremental sync:** ETag fast path; on change, fetch pages until a page contains only already-known `(repo_id, starred_at)` pairs, then stop (list is newest-first).
3. README excerpt queue (kind `readme`): for repos with NULL `readme_excerpt`, `GET /repos/{owner}/{repo}/readme` → base64 decode → strip badges/HTML/code fences → store first 1,500 chars (UTF-8–safe truncate). Runs **in the background after list sync returns**, with **~6 concurrent workers** and a shared **~6 req/s** spacing gate. Resumable via NULL columns; app launch and **Resume READMEs** restart pending work. Soft-fail per repo (encoding `none`, decode errors, most HTTP errors) → write `""` and continue. Hard-pause only on rate limits (403/429) so remaining NULLs can be retried.
4. Progress: emit Tauri event `sync://progress` with `{ kind, current, total, message, error? }`. UI header: Sync / Full sync, progress bar, last-synced relative time, pending README count, Resume READMEs.
5. FTS5 triggers from Phase 0 verified to fire on upserts (unit test: insert repo → FTS match found).

## Out of scope
Categorization, search UI beyond verifying FTS rows exist, insights.

## Acceptance criteria
- [x] Full sync of a real library completes; every repo has non-null `starred_at`.
- [x] Immediate second sync hits ETag 304 path (wiremock + manual).
- [x] Kill app mid-README queue → relaunch / Resume READMEs → continues without re-fetching filled rows.
- [x] Unstarring a repo on GitHub then full-syncing sets `unstarred=1` without deleting the row.

## As-built notes
- DB path (Windows): `%APPDATA%\com.brocode.starboard\starboard.db`
- Settings keys added: `starred_etag`, `last_synced_at`
- Commands: `start_sync`, `resume_readme_queue`, `get_sync_status`
- Deviations from original ~2 req/s serial queue: concurrent background queue (approved during Phase 1)
