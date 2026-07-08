# Task: Phase 3 — LLM Categorization (Ollama)

**Status:** Complete (thinner cut — harden later)  
Read `CLAUDE.md` and `docs/architecture/starboard-hld-and-plan.md` §4.2 first. Depends on Phase 2.

## Goal
Two-pass categorization: user-reviewed taxonomy generation, then batched repo assignment — fully resumable, fully optional at runtime.

## Build

1. `services/ollama.rs` + `services/categorizer.rs` — Ollama client:
   - Base URL + chat model from settings. `GET /api/tags` health check; unreachable → UI shows "Ollama offline".
   - Chat via `POST /api/chat` with JSON-schema `format`, `stream: false`.
2. **Pass 1 — Taxonomy:** sample library → draft → Categories menu → commit. **Edit taxonomy** updates in place (preserves ids/assignments). Force replace clears all assignments.
3. **Pass 2 — Assignment:** batches of 25; mapping to committed taxonomy / `Uncategorized`; never overwrite `source='manual'`; `categorize://progress`.
4. **UI:** Categories nav; category tree DnD; detail picker + re-categorize. Post-sync auto-assign deferred.

## Out of scope (deferred)
Taxonomy diff view, post-sync auto-hook, multi-category per repo, gold-plated UI tests.

## Acceptance criteria
- [x] End-to-end generate → commit → assign with progress UI (manual verify with Ollama + model in Settings).
- [x] Manual override survives assignment / re-categorize blocked for manual.
- [x] Ollama offline disables categorize actions; library still works.
- [x] Mapping + wiremock Ollama tests in Rust.
- [x] Edit taxonomy renames without wiping assignments.

## As-built commands
- `get_ollama_status`, `generate_taxonomy`, `get_taxonomy_edit`, `update_taxonomy`, `commit_taxonomy`, `start_assignment`, `get_categorize_status`, `set_repo_category`, `recategorize_repo`
