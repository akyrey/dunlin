# Dunlin

A desktop database client written in Rust, in the spirit of DBeaver but much smaller.
Product name: **Dunlin**. Crate prefix `dbc-` (see DECISIONS D-012, D-014).

**Status:** documentation complete, implementation not started. See [docs/ROADMAP.md](docs/ROADMAP.md).

## What it does (v1)

- Connects to PostgreSQL, MySQL/MariaDB and SQLite, directly or through an SSH tunnel.
- SQL editor with syntax highlighting and schema-aware autocomplete.
- Schema browser: tables, columns, indexes, constraints, DDL.
- Results as a grid or as JSON, with a viewer for JSON and XML cells.
- Inline editing, row insert and row delete, staged and previewed as SQL before applying.

## Stack

| Layer | Choice |
|---|---|
| Shell | Tauri 2 |
| Backend | Rust workspace (`crates/*`), tokio |
| Drivers | `tokio-postgres`, `mysql_async`, `rusqlite` |
| SSH | `russh` |
| Frontend | React + TypeScript + Vite, CodeMirror 6 |
| Local storage | SQLite file + OS keychain (`keyring`) |

## Documentation

Read in this order:

| Doc | Purpose |
|---|---|
| [docs/PRD.md](docs/PRD.md) | What we are building, for whom, and what is out of scope |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Workspace layout, session model, query execution, storage |
| [docs/DRIVER_API.md](docs/DRIVER_API.md) | Driver traits, value model, schema model, change sets |
| [docs/IPC.md](docs/IPC.md) | Contract between the Rust backend and the frontend |
| [docs/SECURITY.md](docs/SECURITY.md) | Secrets, SSH host keys, TLS, production safeguards |
| [docs/TESTING.md](docs/TESTING.md) | Test strategy, tooling, coverage |
| [docs/DECISIONS.md](docs/DECISIONS.md) | Decision log (ADRs) |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Phases, status, deferred features |
| [docs/phases/](docs/phases/) | One implementation brief per phase |

## Prerequisites

- Rust stable (edition 2024) with `rustfmt` and `clippy`
- Node 22+ and pnpm
- Docker (integration tests only)
- Tauri 2 system dependencies for your OS

## Commands

These exist once Phase 1 is done.

```sh
pnpm --dir apps/desktop tauri dev                           # run the app
cargo test --workspace                                      # unit tests
cargo test --workspace -- --ignored                         # integration tests (Docker)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
pnpm --dir apps/desktop test                                # frontend unit tests
pnpm --dir apps/desktop lint
```
