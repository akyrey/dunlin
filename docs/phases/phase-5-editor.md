# Phase 5: Editor

## Goal

The editor becomes pleasant for daily use: dialect-aware highlighting, schema-aware autocomplete, running the statement under the cursor, multiple tabs, multiple results, history, and confirmation for dangerous statements.

## Read first

PRD FR-2 and FR-6, [ARCHITECTURE.md](../ARCHITECTURE.md#query-execution), [SECURITY.md](../SECURITY.md#production-safeguards). Check the current `@codemirror/lang-sql` documentation for dialects and the schema configuration format.

## In scope

- CodeMirror SQL language per session dialect.
- Autocomplete from `completion_schema`.
- Run selection / statement under cursor / whole script.
- Multiple editor tabs, each with its own connection.
- One result panel per statement of a script.
- Query history.
- Dangerous-statement confirmation.

## Out of scope

Saved queries (Phase 8), formatting SQL, snippets, parameterised queries with prompts, alias-aware column completion beyond what `lang-sql` provides, `EXPLAIN` visualisation.

## Tasks

- [x] 5.1 `completion_schema` in all three drivers: one query per namespace returning table and column names. Conformance test.
- [x] 5.2 `schema_completion` command with caching shared with the Phase 4 metadata cache.
- [ ] 5.3 Editor language setup: map `DriverKind` to a CodeMirror SQL dialect through a registry in `features/editor/dialects.ts` (the one place the frontend maps driver to dialect); light and dark themes.
- [ ] 5.4 Autocomplete: feed the schema to `lang-sql`, set the default schema, reconfigure through a compartment when the cache refreshes. Completes keywords, schemas, tables and `table.column`.
- [ ] 5.5 Statement under cursor: call `sql_split` (debounced, cached per document version), highlight the active statement subtly, resolve the run target as selection, else statement at cursor, else nothing.
- [ ] 5.6 Run script: execute statements sequentially on the tab's connection, stop at first error, show progress (`3 / 12`), allow cancel between and during statements.
- [ ] 5.7 Results panel: one sub-tab per statement with rows or affected count, elapsed time and the statement text; a Messages sub-tab with the log of the run. Errors mark the failing statement in the editor at the reported position.
- [ ] 5.8 Tabs: new, close (confirm if a statement is running), rename, reorder; tab shows connection colour; unsaved editor content is persisted locally and restored on restart.
- [ ] 5.9 Per-tab connection lifecycle: created on first run, closed with the tab (`tab_close`), recreated after `connection_lost`.
- [ ] 5.10 Dangerous statements: backend returns `confirmation_required` with the reason; frontend dialog shows the statement, connection name and colour; retry with `confirmed: true`. For a script, list all dangerous statements once before starting.
- [ ] 5.11 Store migration for `query_history`; record every executed statement with duration, row count or error; cap per connection.
- [ ] 5.12 `history_list` (search, pagination) and `history_clear`; history panel with search, click to insert into the editor, double-click to open in a new tab.
- [ ] 5.13 Keyboard shortcuts: run (`Cmd/Ctrl+Enter`), run script (`Cmd/Ctrl+Shift+Enter`), cancel (`Esc` when running), new tab, close tab, next/previous tab, toggle comment, focus results.
- [ ] 5.14 Row limit selector per tab (100 / 1000 / 10 000 / custom).
- [ ] 5.15 E2E tests: autocomplete offers a table name; run statement under cursor in a three-statement script runs only that one; dangerous statement prompts.

## Acceptance criteria

- Keywords, strings, comments and identifiers are highlighted correctly for each dialect, including PostgreSQL dollar-quoted strings and MySQL backtick identifiers.
- Typing `SELECT * FROM us` offers matching tables; typing `users.` offers its columns.
- With the cursor in the second of three statements, run executes only the second.
- A script with an error in statement 2 of 3 runs the first, reports the second with its position, and does not run the third.
- Two tabs on the same session have independent transactions: `BEGIN` in one is not visible in the other.
- `DELETE FROM t` asks for confirmation; `DELETE FROM t WHERE id = 1` does not.
- Every executed statement appears in history with duration and outcome, and survives a restart.
- Autocomplete stays responsive with a schema of 2000 tables and 30 000 columns.

## Manual smoke

1. Work for an hour in the editor against a real database.
2. Paste a long migration script and run it on a scratch database.
3. Close and reopen the app with several tabs open.
