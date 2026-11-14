# Phase 1: Skeleton

## Goal

A running Tauri app in which the user can add a SQLite connection, open it, type a statement, run it and see rows in a read-only grid. Every layer of the architecture exists, thinly.

## Read first

[ARCHITECTURE.md](../ARCHITECTURE.md), [DRIVER_API.md](../DRIVER_API.md), [IPC.md](../IPC.md), [TESTING.md](../TESTING.md).

## In scope

- Repository, workspace, tooling, CI.
- `dbc-core`: traits, `Value` with wire serialisation, schema model types, `DbError`, statement splitter, statement classifier.
- `dbc-driver-sqlite`: connect, ping, execute with row limit, close. Introspection and editing methods return `Unsupported` for now.
- `dbc-store`: store database with migrations and the `connections` table. No keychain yet (SQLite has no secrets).
- `src-tauri`: `SessionManager`, `AppError`, and the Phase 1 commands from IPC.md.
- Frontend: app layout, connection list and SQLite connection form, one editor tab with a plain CodeMirror editor, run button, read-only virtualised result grid, error display.

## Out of scope

Highlighting configuration and autocomplete (Phase 5), schema tree (Phase 4), anything about PostgreSQL, MySQL, SSH or TLS.

## Tasks

- [x] 1.1 `git init`, `.gitignore`, `rust-toolchain.toml`, workspace `Cargo.toml` with shared lints (`unsafe_code = "forbid"`, clippy `unwrap_used`/`expect_used` denied outside tests), `rustfmt.toml`.
- [x] 1.2 Scaffold `apps/desktop` with Tauri 2, React, TypeScript, Vite, Tailwind, ESLint, Prettier, Vitest. Check the current `create-tauri-app` docs first.
- [x] 1.3 `dbc-core::value`: `Value`, custom `Serialize`/`Deserialize` for the wire format, truncation of large values. Tests: every variant round-trips; `i64::MAX`, `u64::MAX`, `NaN`, infinities, empty string versus null, invalid UTF-8 bytes.
- [x] 1.4 `dbc-core::error`, `schema`, `driver`, `dialect`: types and traits as in DRIVER_API.md. Decide `async-trait` versus native async traits and record it in DECISIONS.md.
- [x] 1.5 `dbc-core::sql::split`. Tests: strings with `;`, escaped quotes, quoted identifiers, backticks, line and block comments, nested block comments (PostgreSQL), dollar quoting with and without tags, trailing statement without `;`, empty statements, property test that ranges never overlap and cover all non-whitespace text.
- [x] 1.6 `dbc-core::sql::classify`. Tests: leading comments and whitespace, CTEs (`WITH ... SELECT` versus `WITH ... DELETE`), `UPDATE`/`DELETE` with and without `WHERE`, `WHERE` appearing only inside a string or subquery, `DROP`, `TRUNCATE`, `EXPLAIN`, `PRAGMA`, unknown input.
- [x] 1.7 Driver conformance suite skeleton in `dbc-core` (`test-support` feature) with the connect, type round-trip and row-limit groups; fixture `sqlite.sql`.
- [x] 1.8 `dbc-driver-sqlite`: worker-thread connection, `connect` (file path, read-only flag, `PRAGMA foreign_keys = ON`), `ping`, `execute`, `close`, interrupt-based `CancelHandle`. Passes the conformance groups from 1.7.
- [x] 1.9 `dbc-store`: open or create the store file, migration runner, `connections` CRUD. Tests against a temp file, including upgrade from an empty file and reopening an existing one.
- [x] 1.10 `src-tauri`: driver registry, `SessionManager`, `AppError` mapping, commands `driver_list`, `connection_*`, `session_open`, `session_close`, `sql_split`, `query_execute`, `tab_close`. Command tests with a temp store and temp SQLite file.
- [x] 1.11 Type generation with `ts-rs` into `src/ipc/generated/`, plus a check that fails when output is stale. Confirm the `Value` type generates correctly (DECISIONS D-010).
- [x] 1.12 Frontend `src/ipc/` wrappers and value formatting helpers (`formatValue`, null rendering). Unit tests.
- [x] 1.13 Frontend: layout shell, connection list, SQLite connection form with file picker and "Test connection".
- [x] 1.14 Frontend: editor tab (CodeMirror, basic setup), run with a button and `Cmd/Ctrl+Enter`, result grid (TanStack Table + Virtual, read-only), affected-rows message, error panel with position, truncated indicator.
- [x] 1.15 Playwright setup with mocked IPC and one E2E test: add connection, open, run query, see rows.
- [x] 1.16 GitHub Actions workflow with the steps in TESTING.md that apply so far.

## Acceptance criteria

- `pnpm --dir apps/desktop tauri dev` opens the app.
- A SQLite file can be added, tested, opened and queried; rows appear in the grid.
- A statement with a syntax error shows the SQLite message without crashing anything.
- `SELECT` returning more than the row limit shows exactly the limit and a truncated marker.
- A 64-bit integer above 2^53 and a `NULL` both display correctly.
- A SQLite connection opened read-only rejects `INSERT`.
- All checks in CI pass; coverage is at or above 80% for `dbc-core`, `dbc-driver-sqlite`, `dbc-store`.

## Manual smoke

1. Create a new SQLite file through the form, create a table, insert rows, select them.
2. Open an existing SQLite database with a few hundred thousand rows and run `SELECT *`: the UI stays responsive and the result is truncated.
3. Quit and restart: the saved connection is still there.
