# Phase 4: Schema browser

## Goal

A sidebar tree of namespaces, tables and views for each open session, and a table detail view showing columns, indexes, constraints and DDL.

## Read first

[DRIVER_API.md](../DRIVER_API.md#schema-model), PRD FR-3.

## In scope

- `list_namespaces`, `list_objects`, `describe_table`, `table_ddl` in all three drivers.
- `schema_*` commands.
- Sidebar tree and table detail tab.
- A per-session metadata cache with manual refresh.

## Out of scope

Functions, procedures, triggers, sequences, types, roles. Editing schema through the UI. Browsing PostgreSQL databases other than the connected one.

## Introspection sources

| | PostgreSQL | MySQL | SQLite |
|---|---|---|---|
| Namespaces | `pg_namespace`, hiding `pg_*` and `information_schema` unless "show system" is on | `information_schema.schemata`, hiding system databases likewise | `PRAGMA database_list` |
| Objects | `pg_class` (tables, views, materialized views, partitioned tables) | `information_schema.tables` | `sqlite_master` |
| Columns | `pg_attribute` + `pg_attrdef` + `format_type` + `col_description` | `information_schema.columns` | `PRAGMA table_xinfo` |
| Keys, constraints | `pg_constraint` | `table_constraints`, `key_column_usage`, `check_constraints` | `PRAGMA index_list`, `foreign_key_list`, parsed `CREATE TABLE` for checks (best effort) |
| Indexes | `pg_index` + `pg_get_indexdef` | `information_schema.statistics` | `PRAGMA index_list`, `index_xinfo` |
| DDL | reconstructed from catalogs | `SHOW CREATE TABLE` | `sqlite_master.sql` |

Use the system catalogs rather than `information_schema` on PostgreSQL: they are faster and complete.

## Tasks

- [x] 4.1 Extend fixtures: composite primary key, multi-column and partial indexes, expression index, self-referencing and composite foreign keys with `ON DELETE` actions, unique and check constraints, generated column, view, column and table comments, identifiers needing quotes.
- [x] 4.2 Conformance group for introspection asserting the full `TableInfo` for each fixture table, including `row_key` selection (primary key, unique not-null fallback, none).
- [x] 4.3 PostgreSQL: `list_namespaces`, `list_objects`, `describe_table`.
- [x] 4.4 PostgreSQL: `table_ddl` reconstruction (columns, defaults, constraints, indexes, comments), labelled as reconstructed. Snapshot tests.
- [x] 4.5 MySQL: all four methods. Test on MySQL and MariaDB (check constraints differ by version).
- [x] 4.6 SQLite: all four methods, including attached databases and `WITHOUT ROWID` tables.
- [x] 4.7 `schema_*` commands using the metadata connection; per-session cache keyed by namespace and table, invalidated by refresh and after any `Ddl`-class statement runs in that session.
- [x] 4.8 Frontend: sidebar tree (session → namespace → Tables / Views → object), lazy loading per node, loading and error states per node, filter box, refresh action, "show system objects" toggle.
- [x] 4.9 Frontend: table detail tab with sub-tabs Columns, Indexes, Constraints (primary key, foreign keys, unique, check), DDL (read-only CodeMirror). Foreign keys link to the referenced table's detail tab.
- [x] 4.10 Context actions on a table: open details, copy name, copy qualified name, generate `SELECT` into a new editor tab.
- [x] 4.11 E2E test: expand tree, open a table, see columns and DDL.

## Notes from implementation

- Until Phase 5 adds editor tabs, "Generate SELECT" appends to the session's one editor and switches to it. The quoting comes from the backend (`schema_snippets`), since the frontend never branches on the driver.
- The tree reads through the session's metadata connection, so a SQLite database `ATTACH`ed in an editor tab is not shown: the attachment belongs to that tab's connection.
- The TypeScript name of `dbc_core::schema::Index` is `TableIndex`, because `Index.ts` would clash with the generated `index.ts` on case-insensitive file systems.

## Acceptance criteria

- The introspection conformance group passes for all three drivers.
- A schema with 2000 tables expands in under a second after the first load and the tree stays responsive (virtualised list).
- Table details are correct for every fixture table, including quoted identifiers and composite keys.
- DDL for MySQL and SQLite is the server's own text; PostgreSQL DDL, when executed on an empty database, recreates a table with the same columns, keys and indexes (round-trip test).
- A long-running query in an editor tab does not block the tree.
- A user without permission on a schema sees a node-level error, not a broken tree.

## Manual smoke

1. Browse a real, large application schema on each database.
2. Create a table in the editor, see it appear after the automatic cache invalidation.
3. Follow a foreign key link from one table to another.
