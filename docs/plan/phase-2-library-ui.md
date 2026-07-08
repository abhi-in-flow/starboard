# Task: Phase 2 — Library UI & Keyword Search

**Status:** Complete (as-built)  
Read `CLAUDE.md` and `docs/architecture/starboard-hld-and-plan.md` §6 (v1) first. Depends on Phase 1.

## Goal
A fast, keyboard-friendly library browser with FTS5 keyword search and filters.

## Build

1. **Library view:** virtualized list/grid (`@tanstack/react-virtual`) — must stay smooth at 5k rows. Row shows: full_name, description (1 line), language dot, star count, starred_at (relative). Sort dropdown: starred_at (default, desc) / stars / pushed_at / name. Toggle: hide unstarred (default on), hide archived. Layout toggle: list + grid.
2. **Detail panel** (right side, opens on select): all repo stats from HLD §3, topic chips (click = filter), README excerpt, license, archived badge, "Open on GitHub" via Tauri shell-open.
3. **Search:** input debounced 150 ms → `search_repos(query, filters)` → FTS5 `MATCH` with rank ordering and prefix matching (append `*` to last token). Empty query uses `list_repos` for filtered browse.
4. **Filter chips row:** language (distinct from DB, with counts), topics, archived/unstarred toggles, category (wired to `repo_categories`; empty until Phase 3).
5. **Category tree sidebar:** reads `categories` (2 levels) with per-node counts; empty state until Phase 3. Click filters library.
6. **Keyboard:** `/` focuses search, `↑/↓` moves selection, `Esc` closes detail / clears query / clears filters.

## Out of scope
Any Ollama call, vector search, insights.

## Acceptance criteria
- [x] Type-to-filter via FTS; DEV console logs round-trip ms.
- [x] Detail panel shows synced fields + README excerpt / pending / empty states.
- [x] Keyboard shortcuts: `/`, ↑/↓, Esc.
- [x] Filters + search + sort compose (`list_repos` / `search_repos`).

## As-built commands
- `list_repos`, `search_repos`, `get_repo`, `get_library_facets`, `list_categories`

## As-built notes
- README excerpt rendered with `react-markdown` + `remark-gfm` + `rehype-raw` + `rehype-sanitize` (safe HTML).
- Separate browse (`list_repos`) vs search (`search_repos`) commands; empty query uses browse.
- Detail panel opens on selection (not Enter); Esc closes detail → clears query → clears filters.
