# Starboard — GitHub Stars Organizer
## High-Level Design & Phased Implementation Plan

**Status:** Handoff-ready for coding agents
**Owner:** Abhilash
**Stack decisions (locked):** Tauri 2.x · React + TypeScript · GitHub PAT auth · Ollama (local LLM on RTX 5090 rig) · Local-only organization (no write-back to GitHub Lists)

---

## 1. Product Summary

A local-first desktop app that:

1. Syncs all of the user's starred GitHub repos (with `starred_at` timestamps) into a local SQLite database.
2. Auto-organizes repos into a two-level taxonomy (categories → subcategories) using a local LLM served by Ollama, with manual override.
3. Provides per-repo stats (stars, forks, issues, language, topics, last push, license, archived status).
4. Supports keyword search (v1, SQLite FTS5) and semantic/vector search (v2, sqlite-vec + local embeddings).
5. v2: Temporal insights — starring patterns by hour/day, interest drift over time, category/language trends.

Non-goals: modifying anything on GitHub (no starring/unstarring, no GitHub Lists sync), multi-user support, cloud sync.

---

## 2. Architecture Overview

```
┌────────────────────────────────────────────────────────┐
│  Tauri Desktop App                                     │
│                                                        │
│  ┌──────────────────┐        ┌──────────────────────┐  │
│  │  React Frontend  │ invoke │  Rust Core (Tauri)   │  │
│  │  (Vite + TS +    │◄──────►│                      │  │
│  │   Tailwind)      │ events │  ├ github_sync       │  │
│  │                  │        │  ├ store (SQLite)    │  │
│  │  - Library view  │        │  ├ categorizer       │  │
│  │  - Detail panel  │        │  ├ search            │  │
│  │  - Search bar    │        │  ├ stats             │  │
│  │  - Category tree │        │  └ settings/keyring  │  │
│  │  - Insights (v2) │        └──────────┬───────────┘  │
│  └──────────────────┘                   │              │
└─────────────────────────────────────────┼──────────────┘
                                          │
                    ┌─────────────────────┼─────────────────┐
                    ▼                     ▼                 ▼
             GitHub REST API       SQLite (app data     Ollama HTTP API
             (api.github.com)      dir, FTS5 +          (configurable URL,
                                   sqlite-vec)          default :11434)
```

### Key architectural decisions

| Decision | Choice | Rationale |
|---|---|---|
| Shell | Tauri 2.x | Small binary, Rust core does sync/DB work off the UI thread |
| Frontend | React 18 + TS + Vite + Tailwind + shadcn/ui | Familiar stack; fast to iterate |
| State | TanStack Query (server-state from Tauri commands) + Zustand (UI state) | Clean cache invalidation after syncs |
| DB | SQLite via `rusqlite` (bundled), migrations via `rusqlite_migration` | Local-first, FTS5 built in, sqlite-vec loadable extension for v2 |
| PAT storage | OS credential store via `keyring` crate | Never store the PAT in SQLite or config files |
| LLM | Ollama HTTP API, **base URL configurable in settings** | App may run on the primary rig while Ollama runs on the SCAR 18 over LAN — do not hardcode localhost |
| LLM output | Ollama structured outputs (`format: <json schema>`) | Reliable parsing for category assignment |
| IPC | Tauri commands for request/response; Tauri events for sync progress streaming | UI shows live progress during long syncs |

### Rust core module layout

```
src-tauri/src/
├── main.rs
├── commands/          # #[tauri::command] handlers (thin, call services)
│   ├── sync.rs
│   ├── repos.rs
│   ├── categories.rs
│   ├── search.rs
│   ├── insights.rs
│   └── settings.rs
├── services/
│   ├── github.rs      # REST client, pagination, ETag cache, rate limits
│   ├── store.rs       # SQLite access layer, migrations
│   ├── categorizer.rs # Ollama client, taxonomy + assignment pipeline
│   ├── search.rs      # FTS5 (v1), hybrid vector (v2)
│   └── insights.rs    # v2 aggregations
└── models.rs          # Serde structs shared across services
```

---

## 3. Data Model (SQLite)

```sql
-- Core repo record. starred_at is captured from day one (needed for v2).
CREATE TABLE repos (
  id            INTEGER PRIMARY KEY,          -- GitHub repo id
  full_name     TEXT NOT NULL UNIQUE,         -- owner/name
  owner         TEXT NOT NULL,
  name          TEXT NOT NULL,
  description   TEXT,
  language      TEXT,
  topics        TEXT,                         -- JSON array
  stars_count   INTEGER,
  forks_count   INTEGER,
  open_issues   INTEGER,
  license       TEXT,
  homepage      TEXT,
  html_url      TEXT NOT NULL,
  archived      INTEGER DEFAULT 0,
  fork          INTEGER DEFAULT 0,
  repo_created_at TEXT,                       -- ISO 8601
  pushed_at     TEXT,
  starred_at    TEXT NOT NULL,                -- from star+json media type
  readme_excerpt TEXT,                        -- first ~1500 chars, lazily fetched
  fetched_at    TEXT NOT NULL,
  unstarred     INTEGER DEFAULT 0             -- soft-delete on sync diff
);

CREATE TABLE categories (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  name      TEXT NOT NULL,
  parent_id INTEGER REFERENCES categories(id), -- NULL = top-level
  UNIQUE(name, parent_id)
);

CREATE TABLE repo_categories (
  repo_id     INTEGER REFERENCES repos(id),
  category_id INTEGER REFERENCES categories(id),
  source      TEXT CHECK(source IN ('llm','manual')),
  confidence  REAL,                            -- LLM self-reported, nullable
  PRIMARY KEY (repo_id, category_id)
);

CREATE TABLE sync_log (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  started_at  TEXT, finished_at TEXT,
  kind        TEXT,                            -- full | incremental | readme | embed
  repos_added INTEGER, repos_updated INTEGER, repos_removed INTEGER,
  status      TEXT, error TEXT
);

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
-- keys: ollama_base_url, ollama_chat_model, ollama_embed_model, github_username

-- Full-text search (v1)
CREATE VIRTUAL TABLE repos_fts USING fts5(
  full_name, description, topics, readme_excerpt,
  content='repos', content_rowid='id', tokenize='porter unicode61'
);
-- Keep in sync with AFTER INSERT/UPDATE/DELETE triggers on repos.

-- v2: embeddings via sqlite-vec
-- CREATE VIRTUAL TABLE repo_embeddings USING vec0(
--   repo_id INTEGER PRIMARY KEY, embedding float[768]
-- );
```

**Design notes**

- `repo_categories` is many-to-many: a repo can live in one primary category but the schema allows multiples; UI treats the highest-confidence/manual one as primary.
- `unstarred` soft-delete preserves history for v2 insights (an unstar is itself a signal).
- Taxonomy depth is capped at 2 (category → subcategory) via app logic, not schema.

---

## 4. External Integrations

### 4.1 GitHub REST API

- **Auth:** `Authorization: Bearer <PAT>` from OS keyring. Classic PAT with no scopes reads public stars; `repo` scope only if private starred repos matter.
- **Critical header:** `Accept: application/vnd.github.star+json` on `GET /user/starred` — this is the only way to get `starred_at`. Without it the whole v2 insights layer is dead. Use it from the first sync.
- **Pagination:** `per_page=100`, follow the `Link: rel="next"` header.
- **Conditional requests:** store the ETag of page 1; on incremental sync, send `If-None-Match` — a `304` means nothing new and costs 0 rate-limit points.
- **Rate limits:** 5,000 req/hr authenticated. Read `x-ratelimit-remaining` / `x-ratelimit-reset` and back off with jitter when `remaining < 50`. A 2,000-star library costs ~20 requests for a full list sync; README fetches are the expensive part.
- **README fetch (lazy, batched):** `GET /repos/{owner}/{repo}/readme` → base64 decode → strip markdown noise → store first ~1,500 chars in `readme_excerpt`. Run as a background queue after list sync, throttled (e.g., 2 req/s), resumable via `sync_log`.
- **Unstar detection:** full sync computes set difference vs local `repos` and flags `unstarred = 1`.

### 4.2 Ollama

- Base URL from settings (LAN-friendly). Health check on app start; degrade gracefully — the app is fully usable without Ollama (categorization/embedding buttons show "Ollama offline").
- **Chat model (categorization):** default `qwen3:14b` or similar mid-size instruct model (fits comfortably in 24GB VRAM with room for KV cache); configurable.
- **Embedding model (v2):** `nomic-embed-text` (768-dim) — matches the `float[768]` vec0 schema above. If the model changes, dimension must change with it (store dimension in settings, migrate table).
- Use `/api/chat` with `format` set to a JSON schema for assignments; `/api/embed` for embeddings.

---

## 5. Categorization Pipeline (two-pass)

**Pass 1 — Taxonomy generation (once, re-runnable):**
Sample the library (all repos if <500; stratified sample by language/topics otherwise). Send name + description + topics to the LLM: "Propose a taxonomy of 8–14 top-level categories, each with 2–6 subcategories, covering these repos. Return JSON." User reviews/edits the taxonomy in a settings screen before it's committed to `categories`.

**Pass 2 — Assignment (batched):**
Batches of ~25 repos per request. Prompt includes the frozen taxonomy and per-repo: full_name, description, language, topics, readme_excerpt (truncated to ~400 chars each). Structured output schema:

```json
{ "assignments": [ { "repo_id": 123, "category": "AI/LLM",
    "subcategory": "Agent Frameworks", "confidence": 0.86 } ] }
```

Rules: unknown category names from the model → map to closest existing via case-insensitive match, else bucket into `Uncategorized` (never silently create taxonomy entries). Writes use `source='llm'`. Manual overrides set `source='manual'` and are **never** overwritten by re-runs.

**Re-categorization triggers:** user-initiated (button), or after a sync adds new repos (only new/uncategorized repos are sent).

---

## 6. Search Design

- **v1 (FTS5):** query `repos_fts` with BM25 ranking, prefix matching (`term*`), filter chips for language / category / topic / archived. Debounced-as-you-type, target <50 ms on 5k repos (trivial for FTS5).
- **v2 (hybrid):** embed `full_name + description + topics + readme_excerpt` per repo → `repo_embeddings`. Query flow: embed query via Ollama → KNN top-50 from sqlite-vec → union with FTS top-50 → Reciprocal Rank Fusion → final ranking. Toggle in UI: Keyword / Semantic / Hybrid (default Hybrid when embeddings exist).

---

## 7. v2 Insights Layer

All computed as SQL aggregations over `starred_at` (this is why capturing it in Phase 1 is non-negotiable):

1. **Starring timeline** — stars per week/month, line chart.
2. **Rhythm heatmap** — hour-of-day × day-of-week matrix (convert `starred_at` UTC → local tz at query time).
3. **Interest drift** — category share per quarter, stacked area chart. Answers "when did I pivot from X to Y".
4. **Language trend** — same but by language.
5. **Interest metrics** — per category: total repos, first/last starred, velocity (stars/month recent vs lifetime), "rising" / "dormant" badges.
6. **Streaks & bursts** — longest starring streak, biggest single-day binge (fun facts panel).

Charting: Recharts.

---

## 8. Phased Implementation Plan

Each phase is a self-contained handoff unit with acceptance criteria. Do not start a phase until the previous one's criteria pass.

### Phase 0 — Scaffold & Foundations (agent-days: ~1)

**Deliverables**
- `create-tauri-app` scaffold: Tauri 2.x + React + TS + Vite; Tailwind + shadcn/ui wired.
- SQLite opened in Tauri app-data dir; migration runner with migration 001 (full schema from §3, minus vec0).
- Settings service + UI stub: Ollama base URL, model names, GitHub username.
- PAT flow: input field → validate via `GET /user` → store in OS keyring; never rendered back in full.
- CI-ish basics: `cargo clippy`, `cargo test`, `eslint`, `tsc --noEmit` all clean.

**Acceptance criteria**
- App launches on Windows 11; DB file created with all tables; PAT survives app restart via keyring; invalid PAT shows a clear error.

### Phase 1 — Sync Engine (agent-days: ~2)

**Deliverables**
- `github.rs`: paginated starred fetch with `star+json` media type, ETag caching, rate-limit backoff.
- Full sync + incremental sync commands; unstar soft-delete diffing; `sync_log` records.
- Background README-excerpt queue (throttled, resumable).
- Tauri event stream `sync://progress` → UI progress bar (page N of M, repos synced, READMEs fetched).
- FTS5 triggers live; index populated on sync.

**Acceptance criteria**
- Full sync of the user's real library completes; every repo row has a non-null `starred_at`.
- Second sync with no changes makes ≤2 network requests (ETag 304 path).
- Killing the app mid-README-queue and relaunching resumes without duplicating work.

### Phase 2 — Library UI & Keyword Search (agent-days: ~2–3)

**Deliverables**
- Library view: virtualized repo grid/list (5k rows smooth), sort by starred_at / stars / pushed_at / name.
- Repo detail panel: all stats from §3, topic chips, README excerpt, "Open on GitHub" (shell open).
- Search bar → FTS5 with prefix match; filter chips: language, category (empty until Phase 3), archived, topic.
- Category tree sidebar (renders empty-state pre-Phase 3).
- Sync button + last-synced indicator in the header.

**Acceptance criteria**
- Type-to-filter feels instant on full library; detail panel matches GitHub's data; keyboard nav (↑/↓ list, Enter opens detail, `/` focuses search).

### Phase 3 — LLM Categorization (agent-days: ~2–3)

**Deliverables**
- Ollama client with health check + offline degradation.
- Taxonomy generation flow with review/edit screen (rename, merge, delete, add before commit).
- Batched assignment pipeline with structured outputs, progress events, resumability.
- Category tree sidebar goes live: counts per node, click-to-filter, drag repo → category (manual override), right-click re-categorize single repo.
- "Categorize new repos" runs automatically post-sync for uncategorized repos only.

**Acceptance criteria**
- Full library categorized end-to-end; zero silent taxonomy mutations; manual overrides survive a full re-categorization run; Ollama offline leaves the rest of the app fully functional.

### Phase 4 — Vector & Hybrid Search (v2a) (agent-days: ~2)

**Deliverables**
- sqlite-vec extension loading; migration 002 adds `repo_embeddings` (dimension from settings).
- Embedding pipeline (batched, resumable, progress-evented) via Ollama `/api/embed`.
- Hybrid search with RRF; Keyword/Semantic/Hybrid toggle; re-embed on repo content change.

**Acceptance criteria**
- "local llm agent memory" surfaces relevant repos that keyword search misses; hybrid query round-trip <500 ms excluding the embedding call; missing embeddings fall back to keyword-only with a hint.

### Phase 5 — Insights Dashboard (v2b) (agent-days: ~2–3)

**Deliverables**
- Insights tab with all six views from §7 (Recharts), date-range selector, per-category drill-down.
- Timezone-correct hour/day heatmap.
- "Fun facts" panel (streaks, bursts, oldest star, first-ever star).
- Export: categorized library → Markdown/JSON (reuses the format from the earlier standalone script).

**Acceptance criteria**
- Charts reconcile with raw SQL spot-checks; date-range filtering applies across all views; empty/short-history states handled gracefully.

### Backlog (post-v2, not scheduled)
- Notes/tags per repo; duplicate-interest detector ("you starred 6 similar RAG frameworks"); stale-star review mode; GitHub Lists write-back via browser automation (explicitly out of scope for now); auto-sync scheduler.

---

## 9. Agent Handoff Conventions

- **Repo layout:** monorepo, `src/` (React) + `src-tauri/` (Rust). One PR-sized branch per phase.
- **Commands are thin:** all logic in `services/`, unit-tested there; commands only marshal types.
- **Types:** mirror Rust models to TS via `ts-rs` (or hand-maintained `types.ts` with a checklist in PR template).
- **Errors:** Rust services return `Result<T, AppError>`; `AppError` serializes to `{ code, message }` for the frontend; no `unwrap()` outside tests.
- **Secrets:** PAT only ever in keyring; assert no PAT string in logs (add a log scrubber test).
- **Testing minimums per phase:** Rust unit tests for sync diffing, rate-limit backoff, taxonomy-mapping rules, RRF merge; one happy-path integration test hitting a mocked GitHub server (`wiremock` crate).
- **Definition of done per phase:** acceptance criteria pass + clippy/eslint/tsc clean + a 5-line CHANGELOG entry.

## 10. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Ollama on a different machine (LAN) is unreachable | Configurable base URL, health check, full offline degradation |
| GitHub secondary rate limits during README queue | Throttle to ~2 req/s, exponential backoff on 403 with `retry-after` |
| Local model produces off-taxonomy categories | Structured outputs + strict mapping to existing taxonomy, `Uncategorized` fallback |
| Embedding model swap breaks vec0 dimension | Dimension stored in settings; migration drops/rebuilds embeddings table |
| FTS/embeddings drift from repos table | Triggers for FTS; embed queue keyed on content hash |
