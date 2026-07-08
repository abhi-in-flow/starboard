# Starboard

Local-first desktop app that syncs your GitHub starred repos into SQLite, auto-categorizes them with Ollama, and provides search + stats.

## Stack

Tauri 2 · React 18 · TypeScript · Bun · SQLite · Tailwind · shadcn/ui · Zustand · TanStack Query · Biome

## Develop

```bash
bun install
bun run tauri dev
```

## Checks

```bash
bun run check
# or individually:
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
bun run lint
bunx tsc --noEmit
```

## Docs

- Architecture: `docs/architecture/starboard-hld-and-plan.md`
- Phase tasks: `docs/plan/`
