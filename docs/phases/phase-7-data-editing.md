# Phase 7: Data editing

## Goal

Edit cells, add rows and delete rows in a table data tab. Changes are staged, shown as SQL, and applied atomically.

## Read first

PRD FR-5 and FR-6, [DRIVER_API.md](../DRIVER_API.md#change-sets), DECISIONS D-005, [SECURITY.md](../SECURITY.md#generated-sql).

## In scope

- `dbc-core::changes`: statement generation.
- `apply_changes` in all three drivers.
- Pending-change state and UI in the grid.
- Preview and apply flow.

## Out of scope

Editing ad-hoc query results, bulk edit ("set column for all selected rows"), paste of multiple cells, undo after apply, conflict detection beyond the one-row check.

## Rules

- Editing is enabled only when `TableInfo.row_key` is present, the object is a table, and the session is not read-only. Otherwise the grid is read-only and says why.
- A row is addressed by the **original** values of its row key.
- Generated and identity-always columns are not editable and are omitted from `INSERT`.
- On insert, a column the user did not touch is omitted so that the database default applies.
- An empty editor means empty string for text columns; `NULL` is set explicitly (shortcut and context action).
- Apply order: deletes, updates, inserts. One transaction. Each `UPDATE` and `DELETE` must affect exactly one row.
- After a successful apply the page is refetched; inserted rows may land on another page, and a notice says how many rows were inserted.
- Pending changes belong to a table tab. Sorting, filtering, paging or refreshing with pending changes asks to apply or discard first.

## Frontend state

```ts
type PendingChanges = {
  updates: Map<RowKeyString, { key: KeyValues; set: Map<ColumnName, Value> }>;
  deletes: Map<RowKeyString, { key: KeyValues }>;
  inserts: Array<{ tempId: string; values: Map<ColumnName, Value> }>;
};
```

A pure reducer over immutable state: editing a cell back to its original value removes the entry; deleting an inserted row removes the insert; deleting an updated row drops the update. This reducer is the most heavily tested frontend code.

## Tasks

- [x] 7.1 `dbc-core::changes`: `ChangeSet` → bound statements and preview strings. Tests per dialect with snapshots: single and composite keys, key column update, `NULL` in set and in key (`IS NULL` handling), every `Value` variant as parameter and as rendered literal, quoting of awkward identifiers, strings containing quotes and backslashes, empty change set.
- [x] 7.2 Property test: for random values, `render_literal` output executed on each real database returns the original value.
- [x] 7.3 `apply_changes` for PostgreSQL (text parameters with casts), MySQL and SQLite (typed binding from `Value`).
- [x] 7.4 Conformance group: insert, update, delete, mixed batch ordering, key update, write `NULL`, constraint violation rolls back everything, row deleted by another connection gives `RowCountMismatch` and rolls back, generated columns skipped, JSON, binary, decimal and timestamp values written and read back exactly.
- [x] 7.5 `changes_preview` and `changes_apply` commands; refuse on read-only sessions and on tables without a row key (checked in the backend, not only the UI).
- [ ] 7.6 Pending-changes reducer with unit tests for every transition and cancellation rule above.
- [ ] 7.7 Cell editors by `TypeCategory`: text input, number input kept as string with validation, boolean toggle, date/time text with format hint, JSON and long text edited in the cell viewer panel with validation, binary read-only (set to `NULL` only). Enter commits, Escape cancels, Tab moves on.
- [ ] 7.8 Visual state: edited cells, inserted rows and deleted rows are marked; original value shown on hover; per-cell and per-row revert.
- [ ] 7.9 Add row (blank, or duplicate selected row without key columns), delete selected rows, set `NULL`.
- [ ] 7.10 Changes bar: counts of pending updates, inserts and deletes, with Preview, Apply and Discard.
- [ ] 7.11 Preview dialog: generated SQL in a read-only editor, connection name and colour, Apply button. Preview is mandatory on first apply in a session and can then be skipped by a setting, except on connections whose profile is flagged "always preview" (a new profile option added in this task).
- [ ] 7.12 Apply flow: success refetches and clears pending changes; failure keeps them, shows the server error and which operation failed, and marks the offending row.
- [ ] 7.13 Guards: prompt on tab close, session close, app quit, sort, filter, page change and refresh while changes are pending.
- [ ] 7.14 E2E tests: edit a cell and apply; insert and delete in one batch; failed apply keeps pending changes; read-only table shows no editors.

## Acceptance criteria

- The change conformance group passes on all three drivers.
- Nothing is written to the database before Apply.
- The SQL in the preview is exactly what is executed, modulo parameters versus literals; a test asserts both are generated from the same structure.
- A batch that fails on its last statement leaves the database unchanged.
- Editing a row that another client changed or deleted is detected through the one-row rule when the key no longer matches.
- Tables without a row key, views and read-only sessions cannot be edited, by UI or by calling the command directly.
- A value containing `'; DROP TABLE x; --` is stored verbatim.
- Large integers, decimals and JSON documents survive an edit round trip byte for byte.

## Manual smoke

1. Fix a typo in a real row; check the preview; apply.
2. Insert a row relying on defaults and a serial/auto-increment key.
3. Violate a foreign key on purpose and confirm nothing was applied.
4. Try to edit on a read-only connection.
