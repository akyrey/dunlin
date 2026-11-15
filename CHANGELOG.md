# Changelog

All notable changes to Dunlin. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/). The version lives in the workspace `Cargo.toml` only.

## [Unreleased]

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

### Security

- Every IPC argument type and the SQL and DDL parsers are fuzzed; CSV options, page offsets and SSH host keys are validated.
- `cargo audit` and `pnpm audit` clean, with accepted advisories recorded in `.cargo/audit.toml`.
