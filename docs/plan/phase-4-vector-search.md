# Task: Phase 4 — Vector & Hybrid Search (v2a)

Read `CLAUDE.md` and `docs/HLD.md` §6 (v2 part) first. Depends on Phase 3.

## Goal
Semantic search via local embeddings (Ollama + sqlite-vec), fused with FTS5 keyword results.

## Build

1. **Migration 002:** load the sqlite-vec extension at connection open; create `repo_embeddings` as `vec0(repo_id INTEGER PRIMARY KEY, embedding float[D])` where `D` comes from settings key `embed_dimension` (default 768 for `nomic-embed-text`). Store `embed_model` + `embed_dimension` used; if the settings model changes, prompt user and rebuild the table via migration path (drop + re-embed).
2. **Embedding pipeline** (kind `embed` in `sync_log`):
   - Document per repo: `"{full_name}\n{description}\nTopics: {topics}\n{readme_excerpt}"`.
   - `POST /api/embed` (batch input, e.g. 32 docs/call). Resumable: repos lacking an embedding row, or whose `content_hash` (add column via this migration: SHA-256 of the document string) changed since last embed.
   - Progress via `embed://progress`; Ollama-offline degradation as usual.
3. **Hybrid search** in `services/search.rs`:
   - Query → embed → sqlite-vec KNN top-50 (`MATCH` + `k=50`) → FTS5 top-50 → Reciprocal Rank Fusion (`score = Σ 1/(60 + rank)`) → merged ranking, filters applied post-fusion.
   - Unit-test the RRF merge with synthetic ranked lists (overlap, disjoint, empty-side cases).
4. **UI:** search-mode toggle `Keyword | Semantic | Hybrid`. Default Hybrid when embeddings exist for >90% of repos, else Keyword with a hint + "Build embeddings" button. Semantic/Hybrid disabled when Ollama offline.

## Out of scope
Insights, re-ranking models, embedding anything beyond the repo document above.

## Acceptance criteria
- Conceptual query (e.g., "local llm agent memory") returns relevant repos in Semantic mode that Keyword mode misses — demonstrate with 3 example queries in the PR description.
- Hybrid round-trip <500 ms excluding the query-embedding call (log timing).
- Changing a repo's description + re-sync marks it stale and re-embeds only that repo.
- Fresh DB runs migrations 001→002 cleanly; Phase 3 DB upgrades cleanly.
