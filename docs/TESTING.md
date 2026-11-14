# Testing

Development is test-first: write a failing test, make it pass with the smallest change, refactor. Target is 80% line coverage per crate and for the frontend.

## Layers

| Layer | What | Tools | Runs |
|---|---|---|---|
| Rust unit | Pure logic: value serialisation, statement splitter and classifier, change-set SQL generation, dialect quoting, store migrations | `cargo test`, `rstest` for tables of cases, `proptest` for the splitter and quoting, `insta` for generated SQL snapshots | always |
| Rust integration | Drivers against real databases; tunnel against a real SSH server; store against a temp file | `testcontainers` (PostgreSQL, MySQL, MariaDB, OpenSSH server), `tempfile` | `cargo test -- --ignored`, needs Docker |
| Command tests | Tauri command functions called directly with a test `SessionManager` and SQLite | `cargo test` | always |
| Frontend unit | Stores, IPC wrappers, value formatting, pending-change reducer, components | Vitest, Testing Library, `@tauri-apps/api/mocks` | always |
| End-to-end | Critical flows through the real UI with a mocked backend | Playwright against the Vite dev server with mocked IPC | CI and before closing a phase |

### Why E2E uses a mocked backend

Tauri's WebDriver support does not cover macOS, which is the primary development platform (**Verify** against current Tauri docs when setting this up in Phase 1). So the two halves are tested separately: Rust command tests prove the backend contract, Playwright proves the UI against that same contract using fixtures. A short manual smoke checklist per phase covers the join.

## Driver conformance suite

One shared test suite, in `dbc-core` behind a `test-support` feature, that every driver must pass. It takes a `Driver` and a way to obtain a fresh database, and checks:

- connect, ping, close; bad credentials give `Auth`; unreachable host gives `Connect`.
- every supported type round-trips to the expected `Value` (per-driver type table).
- `NULL` versus empty string, large integers, high-precision decimals, non-UTF-8 bytes.
- row limit and `truncated`.
- cancellation of a slow query returns `Cancelled` and leaves the connection usable.
- introspection of a fixture schema: columns, primary key, composite keys, indexes, foreign keys, unique and check constraints, views.
- identifiers that need quoting: spaces, quotes, reserved words, mixed case, non-ASCII.
- `fetch_table_page` with sort, filter, offset.
- `apply_changes`: insert, update, delete, key column update, `NULL` writes, rollback on constraint violation, rollback on row-count mismatch.
- read-only sessions reject writes.

Adding a driver means implementing the traits and passing this suite.

## Fixtures

`crates/dbc-core/test-support/fixtures/` holds a small schema per database (`postgres.sql`, `mysql.sql`, `sqlite.sql`) with the same logical content: a few related tables covering every type category, awkward identifiers, a table without a primary key, and a table with a composite key.

## Coverage

- Rust: `cargo llvm-cov --workspace --ignored` (includes integration tests) with a threshold of 80.
- Frontend: Vitest coverage (v8) with a threshold of 80.
- Generated code and `main.rs` wiring are excluded. So is `dbc-core`'s conformance suite when measuring `dbc-core` alone: it is test-support code that runs from the driver crates' tests.
- CI gates `dbc-core`, `dbc-driver-sqlite` and `dbc-store` individually (`cargo llvm-cov --package <crate> --fail-under-lines 80`); more crates join as their phases add them.

## CI (GitHub Actions)

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `cargo test --workspace -- --ignored` (Linux runner with Docker)
5. `cargo audit`
6. `pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm audit`
7. Generated TypeScript types are up to date (regenerate and `git diff --exit-code`)
8. Playwright E2E
9. `pnpm tauri build` on macOS (from Phase 8)

## Test style

- Arrange, Act, Assert, with blank lines between.
- Names describe behaviour: `rejects_update_without_where_on_read_only_session`.
- One behaviour per test. No shared mutable state between tests; each integration test gets its own database or schema.
- Fix the implementation, not the test, unless the test is wrong.

## Manual smoke checklist (per phase)

Each phase brief ends with a short list of things to click through in the real app against real databases. It is done once before the phase is ticked off in the roadmap.
