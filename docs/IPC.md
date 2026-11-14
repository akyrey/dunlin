# IPC contract

The frontend talks to the backend only through Tauri commands. This document is the contract; each phase adds the commands it needs.

## Conventions

- Command names are `snake_case`, prefixed by area: `connection_*`, `session_*`, `query_*`, `schema_*`, `table_*`, `changes_*`, `history_*`, `saved_query_*`, `export_*`, `ssh_*`, `settings_*`.
- Every command takes one argument object and returns `Result<T, AppError>`. On the frontend that is a promise that resolves with `T` or rejects with `AppError`.
- Argument and return types are Rust structs deriving `serde` and `ts-rs`'s `TS`. TypeScript types are generated into `apps/desktop/src/ipc/generated/` and committed; a test fails if they are stale.
- `apps/desktop/src/ipc/` holds one thin typed wrapper per command. Components and stores import from there and never call `invoke` directly.
- IDs (`ConnectionId`, `SessionId`, `TabId`) are UUID strings.
- Database values use the wire format in [DRIVER_API.md](DRIVER_API.md#wire-format).

## Error shape

```ts
type AppError = {
  code: string;        // stable, snake_case: "auth", "syntax", "cancelled", "connection_lost",
                       // "tunnel_lost", "ssh_host_key_unknown", "ssh_host_key_changed",
                       // "read_only", "confirmation_required", "row_count_mismatch", "not_found", "internal", ...
  message: string;     // safe to show to the user
  detail?: unknown;    // code-specific, e.g. { position: 42 } or { host, port, fingerprint }
};
```

Two codes drive UI flows rather than error toasts:

- `ssh_host_key_unknown`: show the fingerprint; on accept call `ssh_trust_host`, then retry `session_open`.
- `confirmation_required`: show the reason from `detail`; on accept retry `query_execute` with `confirmed: true`.

## Commands

| Command | Arguments | Returns | Phase |
|---|---|---|---|
| `driver_list` | none | `DriverInfo[]` (kind, label, capabilities, option fields) | 1 |
| `connection_list` | none | `ConnectionProfile[]` (no secrets) | 1 |
| `connection_save` | `{ profile, secrets? }` | `ConnectionProfile` | 1 |
| `connection_delete` | `{ connectionId }` | none | 1 |
| `connection_test` | `{ profile, secrets? }` | `{ serverVersion, latencyMs }` | 1 |
| `session_open` | `{ connectionId }` | `SessionInfo` (sessionId, driver, capabilities, readOnly) | 1 |
| `session_close` | `{ sessionId }` | none | 1 |
| `sql_split` | `{ sessionId, text }` | `StatementRange[]` | 1 |
| `query_execute` | `{ sessionId, tabId, sql, rowLimit, confirmed? }` | `QueryResult` | 1 |
| `query_cancel` | `{ sessionId, tabId }` | none | 2 |
| `tab_close` | `{ sessionId, tabId }` | none | 1 |
| `ssh_trust_host` | `{ host, port, fingerprint }` | none | 3 |
| `schema_namespaces` | `{ sessionId }` | `Namespace[]` | 4 |
| `schema_objects` | `{ sessionId, namespace }` | `ObjectSummary[]` | 4 |
| `schema_table` | `{ sessionId, table }` | `TableInfo` | 4 |
| `schema_table_ddl` | `{ sessionId, table }` | `string` | 4 |
| `schema_completion` | `{ sessionId, namespace }` | `CompletionSchema` | 5 |
| `history_list` | `{ connectionId, search?, limit, before? }` | `HistoryEntry[]` | 5 |
| `history_clear` | `{ connectionId }` | none | 5 |
| `table_page` | `{ sessionId, table, filter?, sort?, limit, offset }` | `ResultSet` | 6 |
| `table_count` | `{ sessionId, table, filter?, exact }` | `RowCount` | 6 |
| `cell_full_value` | `{ sessionId, table, key, column }` | `Value` | 6 |
| `changes_preview` | `{ sessionId, changeSet }` | `string[]` (SQL) | 7 |
| `changes_apply` | `{ sessionId, changeSet }` | `ApplyReport` | 7 |
| `export_result` | `{ format, columns, rows, path, table? }` | `{ rowsWritten }` | 8 |
| `export_table` | `{ sessionId, table, filter?, sort?, format, path }` | `{ rowsWritten }` | 8 |
| `saved_query_list` / `_save` / `_delete` | | | 8 |
| `settings_get` / `settings_set` | | | 8 |

```ts
type QueryResult = {
  outcome: { kind: "rows"; columns: ColumnMeta[]; rows: Value[][]; truncated: boolean }
         | { kind: "affected"; rows: string };
  elapsedMs: number;
  statementClass: "read" | "write" | "ddl" | "transaction" | "session" | "unknown";
};
```

## Secrets over IPC

Secrets travel only **from** the frontend to the backend, in `connection_save` and `connection_test`. No command returns a secret. `ConnectionProfile` reports only which secrets are stored (`hasDbPassword: true`). Leaving a secret out of `connection_save` keeps the stored one.

## Tauri permissions

The window gets the minimum capability set: the app's own commands, the dialog plugin (file pickers for SQLite files, keys and exports) and the clipboard plugin. No shell, no HTTP, no general filesystem access from the webview. File paths chosen in dialogs are passed to Rust, which does the reading and writing.

## Long-running calls

A command call is one request and one response. Progress for long operations (exports, multi-statement scripts) is reported through a Tauri `Channel` passed as an argument, when a phase needs it. v1 result sets are bounded by the row limit and returned in one response.
