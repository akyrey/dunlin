# Architecture

## Overview

```
┌──────────────────────────── Tauri window ────────────────────────────┐
│  React frontend (apps/desktop/src)                                   │
│  connections · schema tree · editor (CodeMirror) · grid · changes    │
└──────────────────────────────┬───────────────────────────────────────┘
                               │ Tauri commands (docs/IPC.md)
┌──────────────────────────────┴───────────────────────────────────────┐
│  apps/desktop/src-tauri: commands + SessionManager                   │
├───────────────┬────────────────┬──────────────────┬──────────────────┤
│  dbc-store    │  dbc-tunnel    │  dbc-driver-*    │                  │
│  SQLite file  │  russh         │  postgres/mysql/ │                  │
│  + keychain   │  port forward  │  sqlite          │                  │
├───────────────┴────────────────┴──────────────────┤                  │
│  dbc-core: traits, Value, schema model, SQL utils │                  │
└───────────────────────────────────────────────────┴──────────────────┘
```

All database logic is in Rust. The frontend renders and collects input; it never builds SQL except what the user types.

## Workspace

```
Cargo.toml                      workspace
crates/
  dbc-core/                     no I/O, no async runtime dependency beyond traits
    src/value.rs                Value, wire serialisation
    src/schema.rs               Namespace, TableInfo, Column, Index, Constraint
    src/driver.rs               Driver, Connection, CancelHandle traits
    src/dialect.rs              Dialect trait
    src/changes.rs              ChangeSet, statement generation
    src/sql/split.rs            statement splitter
    src/sql/classify.rs         statement classification (safeguards)
    src/error.rs                DbError
  dbc-driver-postgres/
  dbc-driver-mysql/
  dbc-driver-sqlite/
  dbc-tunnel/
  dbc-store/
apps/desktop/
  src-tauri/                    Rust: commands/, session/, error.rs, main.rs
  src/                          TypeScript
    ipc/                        typed wrappers around invoke, generated types
    features/connections/
    features/schema/
    features/editor/
    features/grid/
    features/changes/
    features/history/
    components/                 shared UI primitives
docs/
```

Dependency rule: `dbc-core` ← (`dbc-driver-*`, `dbc-tunnel`, `dbc-store`) ← `src-tauri`. Nothing else.

## Session model

A **connection profile** is saved configuration. A **session** is a profile that has been opened.

```
Session
├── tunnel: Option<Tunnel>            one per session, shared by all its connections
├── metadata connection               schema browser, table grids, applying changes
└── tab connections: Map<TabId, Connection>   created lazily, one per editor tab
```

- One connection per editor tab keeps `BEGIN`/`SET`/temp tables isolated per tab and makes cancellation unambiguous.
- The metadata connection is separate so that a long query in a tab never blocks the schema tree.
- `SessionManager` (in `src-tauri`) owns all sessions behind a `tokio::sync::RwLock<HashMap<SessionId, Arc<Session>>>`. It is the only shared mutable state in the backend.
- Closing a session closes its connections, then its tunnel.
- A dead connection is detected on the next call (or by `ping`), reported as `connection_lost`, and re-established on retry. There is no silent automatic re-execution of a statement.

SQLite: `rusqlite::Connection` is synchronous and not `Sync`. Each SQLite connection lives on its own worker thread and is driven through a channel. Tab connections on the same file are separate handles; WAL mode is not forced on the user's file.

## Query execution

### Ad-hoc statements (editor)

1. Frontend asks the backend to split the script (`sql_split`) and picks the statement(s) to run.
2. Each statement is sent on its own with `query_execute`. Multi-statement scripts run sequentially and stop at the first error.
3. Before sending, `dbc-core::sql::classify` labels the statement. A read-only session rejects writes here, in addition to the server-side guard. Dangerous statements need a `confirmed: true` flag from the frontend (see `docs/SECURITY.md`).
4. The driver reads at most `row_limit` rows. If more are available it stops, releases the remainder, and sets `truncated: true`.
5. Outcome is either a result set or an affected-row count, plus elapsed time.
6. The statement, outcome summary and duration are appended to history.

There is no "load more" for ad-hoc results in v1: the user raises the limit and reruns. Wrapping arbitrary SQL in `LIMIT/OFFSET` is not safe in general.

Abandoning a row stream makes the client drain the whole result, so network drivers cancel the statement once the limit is reached (DECISIONS D-017).

### Table browsing (grid)

The driver's `Dialect` generates `SELECT * FROM <table> [WHERE <user filter>] ORDER BY <sort or primary key> LIMIT n OFFSET m`. The order is always completed with the row key columns, so it is total and pages are stable. When no order column is nullable, the next page seeks past the last row (`WHERE <order> > <last row's values>`) instead of skipping rows with `OFFSET`, so deep pages are as fast as the first (D-033).

### Cancellation

Each connection exposes a `CancelHandle`, usable from another task while a statement runs:

| Driver | Mechanism |
|---|---|
| PostgreSQL | cancel request on a side connection (`cancel_token`) |
| MySQL | `KILL QUERY <connection id>` from the metadata connection |
| SQLite | `sqlite3_interrupt` via rusqlite's interrupt handle |

When a tunnel is in use the cancel side-connection must also go through it.

## Values across the boundary

Database values are converted to `dbc_core::Value` in the driver and serialised to the frontend as tagged objects whose payloads are strings (`{"t":"int","v":"9007199254740993"}`). JavaScript numbers are never used for database numbers. Full definition in [DRIVER_API.md](DRIVER_API.md).

## SSH tunnels

`dbc-tunnel` opens an SSH connection with `russh`, binds a listener on `127.0.0.1:0`, and for each accepted socket opens a `direct-tcpip` channel to the database host and copies bytes both ways. Drivers then connect to `127.0.0.1:<port>` and need no SSH awareness, which keeps every current and future driver compatible.

- TLS to the database through a tunnel must verify against the **real** hostname, not `127.0.0.1`. `ConnectionConfig` therefore carries both the address to dial and the server name to verify.
- A jump host is a second SSH session carried over a `direct-tcpip` channel of the first.
- Keepalives are sent on the SSH connection; if it drops, the session reports `tunnel_lost`.

## Local storage

`dbc-store` keeps one SQLite file in the OS application data directory.

| Table | Contents |
|---|---|
| `connections` | profile fields except secrets, as columns plus a JSON blob for driver-specific options |
| `query_history` | connection id, statement, started at, duration, outcome summary, error |
| `saved_queries` | connection id (nullable), name, text, updated at |
| `known_hosts` | SSH host keys the user accepted in the app |
| `settings` | key/value |

Schema versions are tracked with `PRAGMA user_version` and forward-only migrations. Secrets live in the OS keychain, keyed by connection id and secret kind (`db_password`, `ssh_password`, `ssh_key_passphrase`, `tls_key_passphrase`). Deleting a connection deletes its keychain entries. History is capped (default 10 000 entries per connection) and can be cleared.

## Errors

- Each crate has its own `thiserror` enum. `dbc-core::DbError` is the common driver error and carries a stable `code`, a human message, and optional `position` and server `detail`.
- `src-tauri` maps every error to `AppError { code, message, detail }`, the only error shape the frontend sees.
- Messages shown to the user come from the database where possible; they must not include passwords or full connection strings.

## Frontend

- React + TypeScript + Vite. Zustand for state. Tailwind CSS for styling.
- Editor: CodeMirror 6 with `@codemirror/lang-sql` (dialect and schema configured per session), `lang-json` and `lang-xml` for the cell viewer.
- Grid: TanStack Table with TanStack Virtual, rows and columns virtualised. Reviewed at the start of Phase 6 (see DECISIONS.md, D-006).
- Layout: left sidebar (connections, schema tree), main area with tabs (editor tabs and table tabs), bottom panel (results, messages, pending changes), status bar (connection colour and name, read-only badge, row counts, timing).

## Adding a database later

1. New crate `dbc-driver-<name>` implementing `Driver`, `Connection`, `Dialect`.
2. Register it in the driver registry in `src-tauri`.
3. Add the connection form fields (driven by the driver's declared options) and a CodeMirror dialect.
4. Run the shared driver conformance test suite against it (see TESTING.md).
