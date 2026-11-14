# Project instructions

Desktop database client: Tauri 2 shell, Rust workspace backend, React + TypeScript frontend.
The design lives in `docs/`. Code follows the docs; when they disagree, fix one of them in the same change.

## How to work on a phase

1. Read `docs/ROADMAP.md` to find the current phase, then its brief in `docs/phases/`.
2. Read the design docs the brief links to. Do not re-derive decisions recorded in `docs/DECISIONS.md`.
3. Before using a crate or npm package, check its current docs. The design docs name libraries, not versions or exact APIs, and anything marked **Verify** is an assumption that has not been tested.
4. Work task by task, test first. Tick the task in the phase brief when its tests pass.
5. When a phase's acceptance criteria all pass, tick the phase in `docs/ROADMAP.md`.
6. If the implementation forces a design change, update the design doc and add an entry to `docs/DECISIONS.md`.

## Layout

```
crates/dbc-core              traits, value model, schema model, SQL utilities (no I/O)
crates/dbc-driver-postgres   PostgreSQL driver
crates/dbc-driver-mysql      MySQL/MariaDB driver
crates/dbc-driver-sqlite     SQLite driver
crates/dbc-tunnel            SSH local port forwarding
crates/dbc-store             saved connections, history, saved queries, keychain
apps/desktop/src-tauri       Tauri commands, session manager (thin)
apps/desktop/src             React frontend, organised by feature
```

Dependency rule: `dbc-core` depends on no other workspace crate. Drivers, tunnel and store depend only on `dbc-core`. Only `src-tauri` depends on everything. No driver-specific type crosses a crate boundary.

## Rust conventions

- Errors: `thiserror` enums per crate. No `unwrap`/`expect`/`panic!` outside tests. Never discard an error silently.
- No `unsafe`.
- Async runtime is tokio. Blocking work (rusqlite, keychain) goes to a dedicated thread or `spawn_blocking`.
- Prefer returning new values over `&mut` parameters. Mutation is fine inside a function; shared mutable state lives only in the session manager.
- Files 200-400 lines, 800 hard ceiling. Functions under 50 lines.
- Never build SQL by concatenating values. Values are bound parameters; identifiers go through `Dialect::quote_identifier`.
- Never log secrets, connection strings containing passwords, or cell values.

## Frontend conventions

- TypeScript strict mode. No `any`.
- Types shared with Rust are generated, never hand-written (see `docs/IPC.md`).
- All backend calls go through `src/ipc/`; components never call `invoke` directly.
- State is immutable: Zustand stores replace objects, they do not mutate them.
- No `console.log` in committed code.

## Testing

TDD: failing test, minimal implementation, refactor. 80% line coverage per crate.
Integration tests need Docker and are marked `#[ignore]`. See `docs/TESTING.md`.

## Commits

Conventional commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`, `perf:`, `ci:`). One task per commit where practical.
