# Changelog

All notable changes to Dunlin. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/). The version lives in the workspace `Cargo.toml` only.

## [0.2.2] - 2026-11-25

### Added

- The schema tree and key browser show each connection's colour and database logo; schema tree rows carry a thin edge in their connection's colour.

## [0.2.1] - 2026-11-20

### Added

- Each connection in the sidebar shows its database's logo before its name.

## [0.2.0] - 2026-11-20

### Changed

- The editor, data grid, command palette, shortcuts, update prompt, keychain, updater and logging come from Roost, the kit Dunlin shares with Whimbrel; the release workflow is Roost's shared `release-app.yml`. Behaviour is unchanged.

## [0.1.4] - 2026-11-18

### Added

- A script tab's connection can be changed from the editor toolbar; the tab moves to that connection with its SQL.
- Autocomplete in the table data `WHERE` filter.
- A setting to turn off reconnecting the connections that were open when the app quit.

### Fixed

- JSON and XML are highlighted when the cell viewer opens to edit them.
- No spelling correction in the table filter.

## [0.1.3] - 2026-11-17

### Added

- Right-click an open connection in the sidebar to refresh its schema or close it.

## [0.1.2] - 2026-11-16

### Added

- Redis and Valkey: the key browser groups keys into folders on the `:` separator.

### Changed

- Grid headers show only the column name, so it is not cut off; the type is shown on hover.

## [0.1.1] - 2026-11-16

### Added

- Connections to SQLite, PostgreSQL and MySQL/MariaDB, with TLS modes, passwords in the system keychain, and SSH tunnels with host key verification and an optional jump host.
- Connection folders, manual ordering and duplication.
- SQL editor: highlighting, schema-aware autocomplete, run statement under cursor or whole script, cancel, per-tab connections, results per statement, query history and saved queries.
- Schema browser with tables, views, columns, indexes, constraints and DDL.
- Table data view: paging, sorting, filtering, exact or estimated counts, JSON view, cell viewer for JSON and XML, foreign key navigation.
- Data editing: inline edits, inserts and deletes staged as a change set, previewed as SQL and applied in one transaction; read-only connections and confirmation for unfiltered changes.
- Export of results or whole tables to CSV, JSON or `INSERT` statements, streamed with progress and cancel; copy as TSV, CSV, JSON or `INSERT`.
- Settings (theme, editor font, defaults, history cap, NULL text), command palette, keyboard shortcut registry.
- Window, sidebar, open connections and tabs restored on start.
- Toasts, loading skeletons and per-panel error boundaries.
- Idle sessions are pinged; dead connections reconnect on next use, and an unreachable server is reported until it is back.
- Rotated log files and an "open logs folder" action; the cold start time is shown in Settings.
- App icon.

- Editing the rows of a plain single-table query; manual transaction mode with Commit and Rollback; visual `EXPLAIN` for every driver.
- SSH settings filled from `~/.ssh/config`; MySQL `DELIMITER` scripts; SQLite tables without a key edited by `rowid`; connection export and import without secrets.
- Redis and Valkey: a command console, a key browser, and editing values, TTLs and keys with a preview of the commands.
- Keyset pagination: next and previous pages of large tables seek instead of using `OFFSET`.
- CSV import into a table, with a column mapping, in one transaction.
- Linux packages (deb, rpm, AppImage for x86_64 and arm64); passwords in the Secret Service (GNOME Keyring, KWallet).
- In-app updates: a check at startup (can be turned off in Settings) and "Check for updates" in the command palette and Settings; updates are signature-checked, installed and the app restarts.
- The macOS app is code signed: with a Developer ID when available, otherwise with a self-signed certificate (or ad hoc), so the first launch shows "Open Anyway" instead of "damaged", and saved passwords stay accessible after an update.

### Security

- Every IPC argument type and the SQL and DDL parsers are fuzzed; CSV options, page offsets and SSH host keys are validated.
- `cargo audit` and `pnpm audit` clean, with accepted advisories recorded in `.cargo/audit.toml`.
