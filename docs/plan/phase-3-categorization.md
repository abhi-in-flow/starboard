# Task: Phase 3 — LLM Categorization (Ollama)

Read `CLAUDE.md` and `docs/HLD.md` §4.2 + §5 first. Depends on Phase 2.

## Goal
Two-pass categorization: user-reviewed taxonomy generation, then batched repo assignment — fully resumable, fully optional at runtime.

## Build

1. `services/categorizer.rs` — Ollama client:
   - Base URL + chat model from settings. `GET /api/tags` as health check on app start and before any run; unreachable → UI shows "Ollama offline" state on all categorization actions (CLAUDE.md rule 6).
   - Chat calls via `POST /api/chat` with `format` set to an explicit JSON schema (Ollama structured outputs). `stream: false`.
2. **Pass 1 — Taxonomy generation:**
   - Input sample: all repos if <500, else stratified sample by language+top topics (~400 repos).
   - Prompt: propose 8–14 top-level categories, each 2–6 subcategories, JSON schema `{ categories: [{ name, subcategories: [name] }] }`.
   - Review screen: editable tree (rename / merge / delete / add) → "Commit taxonomy" writes to `categories`. Re-running generation requires explicit confirmation and shows a diff against the existing taxonomy.
3. **Pass 2 — Assignment:**
   - Batches of 25 uncategorized repos. Per repo include: id, full_name, description, language, topics, readme_excerpt truncated to 400 chars.
   - Schema: `{ assignments: [{ repo_id, category, subcategory, confidence }] }`.
   - Mapping rules (CLAUDE.md rule 8): case-insensitive match to committed taxonomy; unmatched → `Uncategorized` (auto-created top-level, the only allowed auto-creation). Write with `source='llm'`.
   - Never touch rows where a `source='manual'` assignment exists (rule 7).
   - Progress via `categorize://progress` events; resumable (uncategorized = no `repo_categories` row).
4. **UI:**
   - Category tree goes live: counts, click-to-filter (already wired in Phase 2).
   - Drag repo → category node = manual override (`source='manual'`, replaces prior primary).
   - Detail panel: current category shown with source badge (🤖/✋) + "Re-categorize" (single-repo LLM call) + manual picker.
   - Post-sync hook: after any sync that adds repos, if Ollama is healthy, auto-run assignment for new repos only (toggle in settings, default on).

## Out of scope
Embeddings/vector search, taxonomy versioning beyond the diff view.

## Acceptance criteria
- Full library categorized end-to-end with progress UI; zero taxonomy entries created by the LLM other than `Uncategorized`.
- Set a manual category on a repo → run full re-categorization → manual assignment unchanged.
- Stop Ollama → all categorization buttons disabled with hint; library/search fully functional.
- Batch containing a repo with no description/README still returns a valid assignment (test with schema-validated mock).
