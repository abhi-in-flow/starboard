# Changelog

## 0.5.0 — Phase 4

- sqlite-vec embeddings (`repo_embeddings` + content-hash staleness) via Ollama `/api/embed`, with `embed://progress` and Build embeddings.
- Hybrid search: FTS5 ∪ KNN merged with Reciprocal Rank Fusion; Keyword | Semantic | Hybrid toggle (defaults to Hybrid at ≥90% coverage).
- Graceful Keyword fallback when Ollama is offline or embeddings are missing; embed dimension rebuild path in Settings.

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
