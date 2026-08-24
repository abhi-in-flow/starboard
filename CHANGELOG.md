# Changelog

## 0.8.0 — Production hardening

- SQLite WAL/FK/busy_timeout/`synchronous=NORMAL`, transactional sync/taxonomy/embed/auth writes, ETag persisted only after apply commits, orphaned `sync_log` reconciliation, and migration 003 indexes plus README/document-hash columns.
- Request timeouts, bounded GitHub rate-limit waits, and cancellable sync/README/embed/categorize jobs with UI Cancel.
- Incremental sync never claims unstar detection; periodic full reconcile; README one-sweep with 404 vs transient retry; cheap embed-status via persisted hashes; paginated browse/search.
- Scrubbed `log` sink at startup, Ollama/Markdown URL sanitization, non-null Tauri CSP, export-dialog path hygiene, Settings DB integrity check, and GitHub Actions CI with Tauri Linux deps (signing/updater remain external).

## 0.7.0

- Semantic/Hybrid search shows a Sorted by relevance indicator and disables the library sort control while a query is active.
- Result rows show a normalized relevance badge (top hit = 100%) for Semantic/Hybrid hits.
- Settings gains a Status tab with embeddings coverage and categorization admin aggregates.

## 0.6.0 — Phase 5

- Insights dashboard: starring timeline, local-TZ rhythm heatmap, interest drift (with subcategory drill-down), language trend, Rising/Dormant/Steady metrics, and fun facts.
- Global date-range filter (all-time / 1y / 6m / custom) applied across all six views; graceful short-history empty states.
- Export categorized library as Markdown or JSON via native save dialog.

## 0.5.0 — Phase 4

- sqlite-vec embeddings via Ollama `/api/embed` (content-hash staleness, `embed://progress`, Build embeddings).
- Auto-embed on launch and after sync once the README queue drains (silent when Ollama is offline).
- Hybrid search with RRF; Keyword | Semantic | Hybrid toggle (Hybrid default at ≥90% coverage).
- Graceful Keyword fallback when Ollama is offline or embeddings are missing; embed-dimension rebuild in Settings.

## 0.4.0 — Phase 3 (thinner cut)

- Categories menu: generate taxonomy via Ollama, edit draft, commit (force replace clears assignments).
- Edit taxonomy: in-place rename/add/remove preserving category ids and assignments.
- Batched assignment with progress; manual overrides never overwritten; detail picker + LLM re-categorize.
- Drag repo → category tree for manual override; Ollama offline disables categorize actions with hint.

## 0.3.0 — Phase 2

- Virtualized library list/grid with sort, hide-unstarred/archived, language/topic filters, and category tree empty state.
- FTS5 keyword search (`search_repos`) plus browse (`list_repos`); detail panel on selection with sanitized Markdown README excerpt and Open on GitHub.
- Keyboard: `/` search, ↑/↓ select, Esc closes detail / clears query / clears filters.
- Library visual polish: owner avatars (`github.com/{owner}.png`), richer repo rows, rounded filter pills, search `/` hint.

## 0.2.0 — Phase 1

- Add GitHub starred-repo sync with `star+json` Accept header, pagination, ETag 304 fast path, and rate-limit backoff.
- Upsert repos with unstar soft-delete + `sync_log`; concurrent background README queue (~6 in-flight / ~6 req/s) with soft-fail and Resume READMEs.
- Emit `sync://progress` (incl. errors); header Sync / Full sync / Resume READMEs with progress bar and last-synced time.

## 0.1.0 — Phase 0

- Scaffold Tauri 2 + React 18 + TypeScript + Vite with Bun, Tailwind v4, shadcn/ui, Zustand, TanStack Query, and Biome.
- Add SQLite migration 001 (full schema minus vec0), settings service, and PAT onboarding via OS keyring.
- Ship app shell (Library / Insights / Settings) with log-scrubber test and clean clippy/test/lint/tsc checks.
