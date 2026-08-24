# Release readiness

This checklist can be completed without embedding signing secrets or updater
endpoints in the repository.

## Can be completed in-repo

- [x] `cargo fmt --check`, clippy (`-D warnings`), and `cargo test`
- [x] `bun install`, `bun run lint`, `bunx tsc --noEmit`, `bun run build`
- [x] GitHub Actions CI (`.github/workflows/ci.yml`)
- [x] SQLite durability PRAGMAs (WAL, foreign_keys, busy_timeout, synchronous=NORMAL)
- [x] Non-null Tauri CSP for local assets + IPC
- [x] PAT stays in the OS keyring; logs are scrubbed
- [x] Settings → Status database integrity check
- [x] Export path checks around the native save dialog (validation/hygiene, not a sandbox)

## Externally blocked (do not invent values)

These require operator-owned secrets or store accounts. Starboard does **not**
ship placeholder signing keys or updater URLs.

- [ ] **Code signing**
  - macOS: Developer ID certificate + notarization credentials
  - Windows: Authenticode certificate
  - Configure via Tauri bundler / CI secrets, not committed files
- [ ] **Auto-updater**
  - Public update endpoint (and optional pubkey) once a release channel exists
  - `tauri.conf.json` `plugins.updater` stays unset until that endpoint is real
- [ ] **Store listings** (optional): Microsoft Store / Mac App Store metadata

## Suggested first unsigned local release

```bash
bun install
cargo test --manifest-path src-tauri/Cargo.toml
bun run tauri build
```

Distribute the unsigned artifact only to trusted machines until signing is
configured outside this repo.
