# Task: Phase 1 — Sync Engine

Read `CLAUDE.md` and `docs/HLD.md` §4.1 first. Depends on Phase 0.

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
3. README excerpt queue (kind `readme`): for repos with NULL `readme_excerpt`, `GET /repos/{owner}/{repo}/readme` → base64 decode → strip badges/HTML/code fences → store first 1,500 chars. Throttle to ~2 req/s. Resumable: queue state derivable from NULL columns, safe to kill and relaunch. 404 (no README) writes empty string, not NULL, so it isn't retried forever.
4. Progress: emit Tauri event `sync://progress` with `{ kind, current, total, message }`. UI header gets a Sync button, progress bar, and "last synced <relative time>".
5. FTS5 triggers from Phase 0 must be verified to fire on these upserts (add a test: insert repo → FTS match found).

## Out of scope
Categorization, search UI beyond verifying FTS rows exist, insights.

## Acceptance criteria
- Full sync of a real library completes; `SELECT COUNT(*) FROM repos WHERE starred_at IS NULL` = 0.
- Immediate second sync makes ≤2 network requests (assert via wiremock request count in test; manually confirm 304 path).
- Kill app mid-README queue → relaunch → queue resumes, no duplicate fetches for filled rows.
- Unstarring a repo on GitHub then full-syncing sets `unstarred=1` without deleting the row.
