# Phase 8: Polish

## Goal

Round off v1: export, saved queries, settings, shortcuts, robustness, and a signed installable build for macOS.

## Read first

PRD FR-7 and the non-functional requirements, [IPC.md](../IPC.md), [SECURITY.md](../SECURITY.md).

## In scope

- Export to CSV, JSON and SQL `INSERT`.
- Clipboard copy formats.
- Saved queries.
- Settings.
- Keyboard shortcut reference and command palette.
- Performance and resilience pass.
- Packaging.

## Out of scope

Everything in the roadmap's deferred list, auto-update, Linux and Windows builds.

## Tasks

### Export

- [x] 8.1 `dbc-core::export`: streaming writers for CSV (RFC 4180, configurable delimiter, header, `NULL` representation), JSON (array of objects, same number rules as the JSON view) and SQL `INSERT` (using `Dialect::render_literal`, batched). Unit tests with awkward values: embedded quotes, newlines, delimiters, unicode, binary, nulls.
- [x] 8.2 `export_result`: writes rows already in the frontend to a path chosen in a save dialog.
- [x] 8.3 `export_table`: streams a whole table (with current filter and sort) from the database to a file without holding it in memory, with progress over a Tauri channel and cancellation. Integration test with a table larger than memory budget for a page.
- [x] 8.4 Frontend: export dialog (format, options, destination), progress, cancel, "reveal in file manager" on completion.
- [x] 8.5 Copy selection as TSV (default), CSV, JSON or `INSERT`; copy with headers.

### Saved queries

- [x] 8.6 Store migration and CRUD for `saved_queries`; commands.
- [x] 8.7 Frontend: save tab as named query, saved queries list in the sidebar (per connection and global), open, rename, delete; a tab opened from a saved query saves back to it.

### Settings

- [x] 8.8 `settings_get`/`settings_set`; settings view: theme (light, dark, system), editor font and size, default row limit, default page size, history cap, preview-before-apply policy, trusted SSH hosts list, `NULL` display text.

### Usability

- [x] 8.9 Command palette (`Cmd/Ctrl+K`) listing actions, open sessions, tables of the active session and saved queries.
- [x] 8.10 Shortcut reference view; all shortcuts defined in one registry.
- [ ] 8.11 Empty states, loading skeletons and error states for every panel; consistent toasts.
- [x] 8.12 Window state (size, sidebar width, open sessions and tabs) restored on start; sessions reconnect lazily.
- [x] 8.13 Connection actions: duplicate, reorder, group by folder.

### Robustness and performance

- [ ] 8.14 Measure cold start (NFR-1) and fix regressions; lazy-load editor language packages and the cell viewer.
- [ ] 8.15 Idle connection handling: periodic `ping` on open sessions, transparent reconnect of idle connections, clear state when a server goes away.
- [ ] 8.16 Audit: every command validates input; every error path has a user-facing message; no panics reachable from IPC (fuzz the command argument deserialisers with malformed input).
- [ ] 8.17 Security checklist from SECURITY.md across the whole codebase; `cargo audit` and `pnpm audit` clean.
- [x] 8.18 Log file rotation and an "open logs folder" action.

### Packaging

- [ ] 8.19 App icon, name, bundle identifier, version from one source.
- [ ] 8.20 `pnpm tauri build` for macOS (universal binary) in CI; code signing and notarisation (needs an Apple Developer account; credentials as CI secrets).
- [ ] 8.21 README: install, first connection, screenshots. Changelog started.

## Acceptance criteria

- Exporting a 5 million row table to CSV completes with flat memory use and can be cancelled.
- CSV produced by the app re-imports into each database with the server's own tool without data change.
- Exported `INSERT` statements run on an empty copy of the table and reproduce the data.
- Saved queries survive restart and round-trip edits.
- Cold start meets NFR-1.
- The app restores its previous window, sessions and tabs.
- The macOS build installs and runs on a machine without developer tools.
- All PRD functional requirements are met; the success criterion (one working week without DBeaver) is attempted and its findings filed as issues.

## Manual smoke

Run the full smoke lists of Phases 1 to 7 once more on the packaged build.
