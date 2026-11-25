# Phase 9: Schema editing

## Goal

Change a table's structure from its detail tab: add, alter, rename and drop columns, manage indexes, keys, foreign keys and checks, rename the table and edit comments. Changes are staged, shown as SQL, and applied in one go.

## Read first

[DRIVER_API.md](../DRIVER_API.md#schema-edits), DECISIONS D-020, D-023, D-042, [SECURITY.md](../SECURITY.md#generated-sql), [Phase 4](phase-4-schema-browser.md) (the detail tab this phase edits).

## In scope

- `dbc_core::schema_edit`: the edit model, validation, shared rendering, and `apply`.
- `Connection::plan_schema_edit` in PostgreSQL, MySQL/MariaDB and SQLite, including SQLite's table rebuild.
- `schema_edit_preview` and `schema_edit_apply` commands.
- Edit mode on the table detail tab (Columns, Indexes, Constraints), with staged changes, preview and apply.

## Out of scope

Creating and dropping tables (the editor does that already; "New table" can reuse this form later). Column order (new columns are appended). Editing views, materialized views, partitions, sequences, triggers, enums and domains. `CREATE INDEX CONCURRENTLY` and MySQL online-DDL options (`ALGORITHM`, `LOCK`). Editing generated expressions (drop and re-add the column). Undo after apply.

## Rules

- An edit is a list of explicit changes against the `TableInfo` the user started from, never a diff of two descriptions: a diff cannot tell a rename from a drop plus an add.
- Dropping or replacing a key, foreign key, check or index names the **original** object as introspected. PostgreSQL and MySQL drop it by name; SQLite, whose constraints are often unnamed, matches it by value. Changing one is a drop plus an add.
- Before applying, the table is described again; if it no longer equals the starting `TableInfo`, the apply is refused ("the table changed since you opened it") and the user reloads.
- The plan is built once. The preview is the plan's text, and apply runs exactly that text.
- Identifiers go through `Dialect::quote_identifier`, comments through `string_literal`. Type names, defaults, check expressions, index expressions and predicates, and `USING` expressions are SQL the user typed, like the grid filter: they are used as written and each must be one expression or type (no `;` outside strings, comments and quotes).
- Refused on read-only sessions, on views, and while the session's metadata connection is in a manual transaction (checked in the backend).
- Every plan that drops something (column, table data via rebuild, key, index, constraint) needs confirmation, through the same `confirmation_required` flow as dangerous statements.
- After an apply, successful or not, the session's schema cache for that namespace is invalidated and the tab reloads the table.

## How each database applies a plan

| | PostgreSQL | MySQL / MariaDB | SQLite |
|---|---|---|---|
| Transaction | One transaction, all or nothing | No DDL transactions: statements run in order and stop at the first failure | One transaction, all or nothing |
| Shape | One `ALTER TABLE` with all column and constraint actions, then `CREATE`/`DROP INDEX`, `COMMENT ON`, and rename last | One `ALTER TABLE` with every clause (atomic on MySQL 8 and on MariaDB's copy algorithm); separate statements only where one `ALTER` cannot hold them, which the preview warns about | Native `ALTER TABLE` (`ADD COLUMN`, `RENAME COLUMN`, `DROP COLUMN`, `RENAME TO`) and `CREATE`/`DROP INDEX` when that is enough; otherwise the table rebuild |
| Alter column | `TYPE ... [USING ...]`, `SET/DROP NOT NULL`, `SET/DROP DEFAULT`, `RENAME COLUMN` | `CHANGE COLUMN old new <full definition>`, the definition being the column's text from `SHOW CREATE TABLE` with only the changed clauses replaced (see below) | Rebuild |
| Comments | `COMMENT ON TABLE/COLUMN` | Column `COMMENT` in the definition; table `COMMENT =` | Not supported (no comments in SQLite); the UI hides the field |

### MySQL column definitions

`CHANGE COLUMN` restates the whole column, and `TableInfo` does not hold all of it: `AUTO_INCREMENT`, `ON UPDATE CURRENT_TIMESTAMP`, character set, collation, `INVISIBLE` and `SRID` are not in the model, and MySQL reports a string default unquoted (`none`) where MariaDB quotes it (`'none'`). A definition rebuilt from `TableInfo` would drop those attributes or write a wrong default. So the planner reads `SHOW CREATE TABLE`, splits the column's line into clauses, and replaces only the clauses the user changed (type, nullability, default, comment); the others are kept as the server wrote them. A changed type drops the old character set and collation, since they may not apply to the new type. Comments are written with `Dialect::render_literal`, which means the same in every `sql_mode` (D-023).

### SQLite table rebuild

The procedure from the SQLite manual ("Making Other Kinds Of Table Schema Changes"):

1. Outside the transaction: `PRAGMA foreign_keys = OFF` if it was on (otherwise dropping the old table would fire `ON DELETE` actions on child rows).
2. `BEGIN`.
3. `CREATE TABLE` a temporary name with the new definition.
4. `INSERT INTO new (kept columns) SELECT kept columns FROM old`; a changed type converts through SQLite's affinity, as `CAST` would.
5. `DROP TABLE old`, `ALTER TABLE new RENAME TO old`.
6. Recreate the table's indexes and triggers from their stored `CREATE` text (dropped or added ones excepted).
7. `PRAGMA foreign_key_check(table)`; any row fails the plan.
8. `COMMIT`, or `ROLLBACK` on any failure.
9. Always, after commit or rollback: restore `PRAGMA foreign_keys` to its previous value.

The new definition is the original `CREATE TABLE` text edited piece by piece, not regenerated from `TableInfo`: the text is split into column definitions, table constraints and table options (`WITHOUT ROWID`, `STRICT`), unchanged pieces are kept byte for byte, so collations, `AUTOINCREMENT`, conflict clauses, `DEFERRABLE` and generated columns survive.

Notes from implementation:

- The copy is renamed back with `PRAGMA legacy_alter_table = ON`: since SQLite 3.26 a rename checks every view and trigger in the schema, and views on the table fail that check while the old table is gone. The setting is restored in `after`.
- The rebuild keeps the columns' original names. Column and table renames run last, natively, with `legacy_alter_table` back off, so SQLite itself rewrites indexes, triggers, views and other tables' foreign keys. A new column cannot take the old name of a column renamed in the same edit. Expressions in new checks and indexes must use the old names when the same edit renames those columns.
- The `AUTOINCREMENT` counter is copied through `sqlite_sequence` before the old table is dropped.
- The foreign key check is a `SchemaCheck` (a query that must return no rows), run before the commit and only when `foreign_keys` was on.
- Adding a `NOT NULL` column without a default, or one with an expression or `CURRENT_*` default, rebuilds the table, since `ADD COLUMN` refuses those.

## Tasks

- [ ] 9.1 Fixtures: a table with every kind of column, key, index and constraint per database (reuse the Phase 4 fixtures), and SQLite tables with `AUTOINCREMENT`, `WITHOUT ROWID`, `STRICT`, collations, generated columns, triggers and a child table with `ON DELETE CASCADE`.
- [x] 9.2 `dbc_core::schema_edit`: `SchemaEdit`, `SchemaChange`, `SchemaPlan`; validation (unknown column, duplicate name, empty key, dropping a column a kept key uses, user SQL with a `;`), pure, with unit tests.
- [x] 9.3 `dbc_core::schema_edit::apply`: transactional plans in one transaction; non-transactional plans stop at the first failure and report which statements ran; `before`/`after` statements around the transaction, `after` always runs. Tests against the fake connection.
- [x] 9.4 PostgreSQL `plan_schema_edit`, with `insta` snapshots for every change kind, awkward identifiers and combinations.
- [x] 9.5a `dbc_core::sql`: split a `CREATE TABLE` body into column definitions, table constraints and table options, and a column definition into its clauses, keeping every piece's text as written. Shared by the MySQL and SQLite planners.
- [x] 9.5 MySQL/MariaDB `plan_schema_edit` from `SHOW CREATE TABLE`, snapshots per flavour (pure planner over the `CREATE` text).
- [x] 9.6 SQLite: split a `CREATE TABLE` into pieces (shared with MySQL, 9.5a); property test that splitting and joining an unchanged definition gives the original text. Renames need no token rewriting: see the rebuild notes.
- [x] 9.7 SQLite `plan_schema_edit`: native statements where they suffice, the rebuild otherwise; foreign keys, triggers and indexes survive (in-process tests, no Docker).
- [ ] 9.8 Conformance group "schema edit": every change kind is applied, then the table is described again and equals the expected `TableInfo`; a failing change rolls back everything (PostgreSQL, SQLite) or stops and reports the applied statements (MySQL); a stale `TableInfo` is refused.
- [x] 9.9 `Capabilities::schema_editing`; `schema_edit_preview` and `schema_edit_apply` commands with the backend checks under Rules; cache invalidation.
- [x] 9.10 Frontend: pending schema changes as a pure reducer over immutable state (add, change, revert, drop an added item removes it), with unit tests for every transition.
- [x] 9.11 Frontend: Edit mode on the detail tab. Columns: inline name, type (free text with the driver's common types suggested), nullable, default, comment; add and drop. Indexes: add (columns, unique, method, predicate) and drop. Constraints: primary key, unique, foreign key (referenced table picker from the tree), check; add and drop. Changed rows are marked, with per-item revert.
- [x] 9.12 Changes bar, preview dialog (SQL, warnings, connection colour), confirmation for drops, apply, error with the failing statement marked, and the "applied so far" list for MySQL.
- [x] 9.13 Guards: prompt on tab close, session close and refresh while schema changes are pending.
- [ ] 9.14 E2E tests: add a column and apply; drop an index with confirmation; a failed apply keeps the pending changes.

## Notes from frontend implementation

- Staged changes live in `useSchemaEdits` by view key, because the detail tab is unmounted while hidden; their count goes to the edit store, so the tab-close, disconnect and quit guards cover them as they cover row edits.
- The referenced table of a new foreign key is typed as `schema.table` (or `table`, in the same schema), not picked from the tree.
- The UI does not know which databases support comments or index methods; the backend refuses them in the preview (SQLite).
- After a plan without a transaction fails part-way, the staged changes are cleared and the table reloaded: they were made against a table that no longer exists in that form.

## Acceptance criteria

- The schema edit conformance group passes on PostgreSQL, MySQL, MariaDB and SQLite.
- On SQLite, a rebuild keeps every row, every index and trigger, `AUTOINCREMENT` counters and the table options, and never deletes rows from child tables.
- On PostgreSQL and SQLite a failing plan leaves the table exactly as it was. On MySQL it stops at the first failing statement and the UI lists the statements that were applied.
- The preview shows exactly the statements that run, and names every drop.
- A read-only session, a view, and a stale description are refused by the backend.

## Manual smoke

1. On each database: add a column with a default, rename another, change a type, add a foreign key, apply, and check the data grid.
2. On SQLite, change a column's type in a table with a trigger and a child table; check the trigger still fires and no child rows were lost.
3. On MySQL, make the second of two statements fail and read the report.
