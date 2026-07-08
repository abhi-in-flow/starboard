# Task: Phase 2 — Library UI & Keyword Search

Read `CLAUDE.md` and `docs/HLD.md` §6 (v1 part) first. Depends on Phase 1.

## Goal
A fast, keyboard-friendly library browser with FTS5 keyword search and filters.

## Build

1. **Library view:** virtualized list/grid (`@tanstack/react-virtual`) — must stay smooth at 5k rows. Row shows: full_name, description (1 line), language dot, star count, starred_at (relative). Sort dropdown: starred_at (default, desc) / stars / pushed_at / name. Toggle: hide unstarred (default on), hide archived.
2. **Detail panel** (right side, opens on select): all repo stats from HLD §3, topic chips (click = filter), README excerpt, license, archived badge, "Open on GitHub" via Tauri shell-open.
3. **Search:** input debounced 150 ms → command `search_repos(query, filters)` → FTS5 `MATCH` with BM25 ordering and prefix matching (append `*` to last token). Empty query = filtered browse. Filters compose as SQL WHERE with the FTS join.
4. **Filter chips row:** language (distinct from DB, with counts), topics, archived, category (renders but empty until Phase 3 — wire to `repo_categories` now).
5. **Category tree sidebar:** component reads `categories` (self-join for 2 levels) with per-node counts; shows an empty state ("No categories yet — run categorization in Phase 3"). Click filters library.
6. **Keyboard:** `/` focuses search, `↑/↓` moves selection, `Enter` opens detail, `Esc` closes/clears.

## Out of scope
Any Ollama call, vector search, insights.

## Acceptance criteria
- Type-to-filter feels instant on the full library (search round-trip <50 ms measured via console timing on 2k+ rows).
- Detail panel data matches GitHub's page for 3 spot-checked repos.
- All keyboard shortcuts work; virtualized scroll has no blank flashes.
- Filters + search + sort compose correctly (e.g., language=Rust + query "async" + sort stars).
