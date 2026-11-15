# Product requirements

## Problem

DBeaver does everything, and is heavy and slow to start because of it. The goal is a fast, small client that covers the daily loop of a developer working with relational databases: connect, look at the schema, run queries, inspect and fix data.

## User

A single developer on their own machine, comfortable with SQL, working against local, staging and production databases, some of them reachable only through an SSH bastion.

## Goals

1. Cover the daily loop for PostgreSQL, MySQL/MariaDB and SQLite, and for Redis/Valkey key-value stores (D-032).
2. Make it hard to damage a production database by accident.
3. Make adding a fourth database a matter of writing one driver crate.
4. Start in about a second and stay responsive on large results.

## Non-goals for v1

- Database administration: users, roles, grants, server configuration, backups.
- ER diagrams, schema diff, visual query builder, visual `EXPLAIN`.
- Data import.
- Editing schema through forms (DDL is written by hand in the editor).
- Team features, sync, cloud accounts.
- Reading `~/.ssh/config`. SSH settings are entered per connection.
- Stored procedure debugging.

## Functional requirements

### FR-1 Connections

- FR-1.1 Create, edit, duplicate, delete and test saved connections.
- FR-1.2 PostgreSQL and MySQL: host, port, database, user, password, TLS mode, CA certificate, client certificate and key.
- FR-1.3 SQLite: file path, with a read-only option.
- FR-1.4 Optional SSH tunnel per connection: host, port, user, and authentication by agent, private key (with optional passphrase) or password. One optional jump host.
- FR-1.5 Unknown SSH host keys prompt for trust; changed host keys are refused.
- FR-1.6 Passwords and passphrases are stored in the OS keychain only.
- FR-1.7 Each connection has a name, a colour tag and a **read-only** flag.
- FR-1.8 Several connections can be open at once.

### FR-2 SQL editor

- FR-2.1 Syntax highlighting for the dialect of the active connection.
- FR-2.2 Autocomplete for keywords, schemas, tables and columns.
- FR-2.3 Run the selection, or the statement under the cursor, or the whole script.
- FR-2.4 Multiple editor tabs; each tab has its own database connection, so session state and transactions do not leak between tabs.
- FR-2.5 Cancel a running query.
- FR-2.6 Results are capped at a configurable row limit (default 1000) and marked as truncated when the cap is hit.
- FR-2.7 Errors show the server message and, where the server provides it, the position in the statement.
- FR-2.8 Every executed statement is recorded in a searchable history, per connection.
- FR-2.9 Queries can be saved with a name and reopened.

### FR-3 Schema browser

- FR-3.1 Tree of namespaces (databases/schemas) and their tables and views.
- FR-3.2 Per table: columns (type, nullability, default, comment), primary key, indexes, foreign keys, unique and check constraints.
- FR-3.3 DDL view for a table.
- FR-3.4 Manual refresh.

### FR-4 Data viewing

- FR-4.1 Open a table as a paged grid (default 200 rows per page).
- FR-4.2 Sort by clicking a column; filter with a `WHERE` expression. Both run on the server.
- FR-4.3 Show an estimated row count immediately and an exact count on request.
- FR-4.4 Toggle any result between grid view and JSON view.
- FR-4.5 Cell viewer panel: pretty-printed, highlighted JSON and XML; hex and size for binary; full text for long strings.
- FR-4.6 `NULL` is visually distinct from an empty string.
- FR-4.7 Clicking a foreign key value opens the referenced row.
- FR-4.8 Large integers and decimals are shown exactly, with no loss of precision.

### FR-5 Data editing

- FR-5.1 Edit cells inline, add rows, delete rows, set a cell to `NULL`.
- FR-5.2 Edits are staged as pending changes and highlighted; nothing reaches the database until the user applies them.
- FR-5.3 Before applying, the user sees the exact SQL that will run.
- FR-5.4 Changes apply in a single transaction. If any statement fails, or an `UPDATE`/`DELETE` does not affect exactly one row, everything rolls back.
- FR-5.5 Pending changes can be discarded individually or all at once.
- FR-5.6 A table without a primary key or a unique not-null key is read-only.
- FR-5.7 Results of ad-hoc queries are read-only in v1; editing is available on tables opened from the schema browser.

### FR-6 Safeguards

- FR-6.1 A read-only connection rejects every write, enforced on the server session where the database supports it.
- FR-6.2 `UPDATE` or `DELETE` without `WHERE`, `DROP` and `TRUNCATE` require confirmation.
- FR-6.3 The colour tag of the active connection is always visible.

### FR-7 Export

- FR-7.1 Export a result or a table page as CSV, JSON or SQL `INSERT` statements.
- FR-7.2 Copy selected cells or rows to the clipboard as TSV or JSON.
- FR-7.3 Import a CSV file into a table, mapping its fields to columns, in one transaction (after v1, D-034).

## Non-functional requirements

| ID | Requirement |
|---|---|
| NFR-1 | Cold start to usable window in under 1.5 s on the development machine |
| NFR-2 | The UI never blocks on a database or network call |
| NFR-3 | Scrolling a 1000-row, 50-column result stays smooth (virtualised rendering) |
| NFR-4 | No secret is written to disk outside the OS keychain, or to logs |
| NFR-5 | Runs on macOS and Linux (deb, rpm, AppImage; D-035); nothing in the design prevents Windows |
| NFR-6 | A new driver requires no change to `dbc-core` traits or to the frontend beyond registering the dialect |

## Success criteria

The author uses it instead of DBeaver for a full working week across all three databases, including at least one database behind an SSH bastion, without falling back.
