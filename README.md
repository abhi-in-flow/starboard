# Starboard

Local-first desktop app for organizing GitHub stars.

I tend to star interesting repositories while browsing GitHub, Hacker News, Reddit, Twitter, and blog posts. After a few months, those stars turn into a giant pile of bookmarks that I rarely revisit.

GitHub Lists help, but I almost never remember to organize repositories as I starred them. I initially tried using Claude in the browser to automate categorization and maintain GitHub Lists, but it quickly became too slow and wasn't something I could rely on long term.

Starboard is my attempt to solve that problem properly.

Instead of organizing GitHub itself, it keeps a local copy of my starred repositories, enriches them with metadata, automatically categorizes them using a local LLM, and makes them much easier to search, browse, and rediscover.

Starboard is a local desktop application. GitHub remains the source of truth for your stars, while all organization and metadata are managed locally.

## Screenshots


<p align="center">
  <img src="./docs/screenshots/1.png" alt="Starboard screenshot" width="1000" />
</p>

<p align="center">
  <img src="./docs/screenshots/2.png" alt="Starboard screenshot" width="1000" />
</p>

<p align="center">
  <img src="./docs/screenshots/3.png" alt="Starboard screenshot" width="1000" />
</p>
---

## Current Features

- Sync GitHub starred repositories into a local SQLite database
- Capture the when you starred and the readme among other details
- Fast keyword search using SQLite FTS5
- Automatic categorization using Ollama (I'm using gemma4:e4b, smaller or other models might fail).
- Manual category overrides
- Rich repository metadata
- Local-first architecture
- GitHub remains read-only (no write-back)

---

## Roadmap

The project is being built incrementally.

### Current

- GitHub sync
- Repository library
- AI categorization

### Next

- Semantic / vector search
- Better repository discovery
- Smarter filtering and navigation

### Future ideas

One thing I'm particularly interested in building is an **Insights** page that answers questions like:

- When do I usually star repositories?
- What am I actually interested in?
- How have my interests changed over time?
- Which technologies am I exploring more?
- Which repositories have I forgotten about?

Beyond that, I'm intentionally keeping the roadmap flexible.

This project is primarily built to solve my own workflow, but I'm curious to see how other people end up using it. If there are ideas that genuinely improve the experience of managing and rediscovering GitHub stars, I'm open to them. Contributions, discussions, and feature suggestions are always welcome.

---

## Tech Stack

- Tauri 2
- React 18
- TypeScript
- Bun
- SQLite
- Tailwind CSS
- shadcn/ui
- Zustand
- TanStack Query
- Biome
- Ollama

---

## Development

```bash
bun install
bun run tauri dev
```

---

## Checks

Run everything:

```bash
bun run check
```

Or individually:

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
bun run lint
bunx tsc --noEmit
```

---

## Data

### SQLite

```
%APPDATA%\com.brocode.starboard\starboard.db
```

### GitHub Personal Access Token

Stored securely in the operating system credential store.

```
Service: starboard
Account: github_pat
```

The PAT is **never** written to SQLite or configuration files.

### Reset Database

1. Quit Starboard
2. Delete:

```
starboard.db
starboard.db-wal
starboard.db-shm
```

3. Launch the app again

---

## Documentation

- Architecture → `docs/architecture/starboard-hld-and-plan.md`
- Implementation Plan → `docs/plan/`

---

## Project Status

- ✅ Phase 0 — Scaffold
- ✅ Phase 1 — GitHub Sync
- ✅ Phase 2 — Repository Library
- ✅ Phase 3 — AI Categorization
- 🚧 Phase 4 — Vector Search
- 📋 Phase 5 — Insights

---

## Design Principles

A few design decisions that are unlikely to change:

- GitHub is the source of truth for starred repositories.
- Organization happens locally and never modifies GitHub.
- The GitHub PAT stays in the OS credential store.
- Manual categories always take precedence over AI-generated ones - Unless a better and more flexible taxonomy generation engine is added.
- Your repository data stays local.

Today, Starboard uses Ollama for categorization because I wanted everything to work locally by default. I'd eventually like to support additional providers (OpenAI, Anthropic, and OpenAI-compatible APIs), allowing users to choose whichever model they prefer while keeping Starboard itself a local desktop application.
