# CLAUDE.md — Starboard

Local-first Tauri desktop app that syncs the user's GitHub starred repos into SQLite, auto-categorizes them with a local LLM (Ollama), and provides search + stats. Full design lives in `docs/architecture/starboard-hld-and-plan.md` (stub: `docs/HLD.md`) — read the relevant section before starting any task.

## Stack (locked — do not substitute)

- **Shell:** Tauri 2.x
- **Frontend:** React 18 + TypeScript + Vite + Tailwind + shadcn/ui; TanStack Query for Tauri-command data, Zustand for UI state
- **Core:** Rust; SQLite via `rusqlite` (bundled feature) + `rusqlite_migration`; FTS5 for search; sqlite-vec in Phase 4
- **Secrets:** `keyring` crate (OS credential store) — the GitHub PAT never touches SQLite, config files, or logs
- **LLM:** Ollama over HTTP; base URL comes from settings (may be a LAN address, never hardcode localhost)
- **Charts (Phase 5):** Recharts

## Repo layout

```
src/                  # React app
  components/  views/  lib/  types.ts
src-tauri/src/
  main.rs
  commands/           # #[tauri::command] handlers — thin marshaling only
  services/           # ALL business logic lives here, unit-tested here
  models.rs
docs/architecture/starboard-hld-and-plan.md  # source of truth for design
tasks/                # phase task prompts
```

## Hard rules

1. **Commands are thin.** No logic in `commands/`; they validate input, call a service, map errors. If a command exceeds ~30 lines, extract to a service.
2. **Errors:** services return `Result<T, AppError>`. `AppError` serializes to `{ code, message }` for the frontend. No `unwrap()`/`expect()` outside tests and `main.rs` startup.
3. **Secrets:** PAT only via keyring. Never log headers. There is a log-scrubber test — keep it passing.
4. **DB changes only via migrations.** Never `ALTER TABLE` ad hoc; add a numbered migration.
5. **`starred_at` is sacred.** The starred-repos fetch MUST use `Accept: application/vnd.github.star+json`. Any change to `github.rs` must keep a test asserting this header.
6. **Ollama is optional at runtime.** Every feature that touches Ollama must degrade gracefully when it's unreachable (disabled buttons + hint, never a crash or blocking spinner).
7. **Manual category overrides (`source='manual'`) are never overwritten** by LLM re-categorization.
8. **No new taxonomy entries from the LLM.** Off-taxonomy names map to closest existing (case-insensitive) or `Uncategorized`.
9. **Types stay mirrored.** Any change to a serialized Rust model must update `src/types.ts` in the same commit.
10. **Stay in scope.** Do only what the current task file specifies. No opportunistic refactors, no extra dependencies without noting why in the PR description.

## Commands

```bash
# dev
bun install && bun run tauri dev
# checks — ALL must be clean before done
cargo fmt    --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test   --manifest-path src-tauri/Cargo.toml
bun run lint && bunx tsc --noEmit && bun run build
```

## Testing minimums

- Unit tests in `services/` for: sync diffing (incl. unstar soft-delete), rate-limit backoff, taxonomy mapping rules, RRF merge (Phase 4).
- One integration test per network-touching service against a `wiremock` mock server. Never hit real GitHub or Ollama in tests.
- UI: happy-path component tests only; don't gold-plate.

## Definition of done (every task)

- [ ] Acceptance criteria in the task file all pass (manually verify + note how)
- [ ] clippy / cargo test / eslint / tsc clean
- [ ] Migrations run cleanly on a fresh DB **and** on a DB from the previous phase
- [ ] `CHANGELOG.md` entry (≤5 lines)
- [ ] No PAT or Authorization header in any log output
