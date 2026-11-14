# IPC contract

The frontend talks to the backend only through Tauri commands. This document is the contract; each phase adds the commands it needs.

## Conventions

- Command names are `snake_case`, prefixed by area: `connection_*`, `session_*`, `query_*`, `schema_*`, `table_*`, `changes_*`, `history_*`, `saved_query_*`, `export_*`, `ssh_*`, `settings_*`.
- Every command takes one argument object and returns `Result<T, AppError>`. On the frontend that is a promise that resolves with `T` or rejects with `AppError`. On the Rust side the object is a single `args` parameter (`invoke("sql_split", { args: { sessionId, text } })`); the wrappers in `src/ipc/` hide the envelope.
- Argument and return types are Rust structs deriving `serde` and `ts-rs`'s `TS`. TypeScript types are generated into `apps/desktop/src/ipc/generated/` and committed; a test fails if they are stale.
- `apps/desktop/src/ipc/` holds one thin typed wrapper per command. Components and stores import from there and never call `invoke` directly.
- IDs (`ConnectionId`, `SessionId`, `TabId`) are UUID strings.
- Database values use the wire format in [DRIVER_API.md](DRIVER_API.md#wire-format).
- `query_execute` refuses text containing more than one statement (`invalid_argument`): safeguards classify one statement, and MySQL would otherwise run a whole script sent at once.
- A tab connection that reports `connection_lost` is dropped; the next `query_execute` on that tab reconnects. Statements are never re-run automatically.
- Text offsets (`StatementRange`, error `position`) are UTF-16 code units, as JavaScript and CodeMirror count. An error position is relative to the statement that was sent.
- Regenerate types with `pnpm --dir apps/desktop gen:types`; `cargo test -p dunlin bindings` fails if they are stale.

## Error shape

```ts
type AppError = {
  code: string;        // stable, snake_case: "auth", "syntax", "cancelled", "connection_lost",
                       // "tunnel_lost", "ssh_host_key_unknown", "ssh_host_key_changed",
                       // "read_only", "confirmation_required", "row_count_mismatch", "not_found",
                       // "invalid_argument", "unsupported", "store_too_new", "internal", ...
  message: string;     // safe to show to the user
  detail?: unknown;    // code-specific, e.g. { position: 42 } or { host, port, fingerprint }
};
```

Two codes drive UI flows rather than error toasts:

- `ssh_host_key_unknown`: show the fingerprint (`detail: { host, port, algorithm, fingerprint }`); on accept call `ssh_trust_host`, then retry `session_open`.
- `ssh_host_key_changed`: shown as an error with no way to proceed; the user removes the old key in Settings (or their `~/.ssh/known_hosts`) deliberately.
- `tunnel_lost`: the backend has closed the session; the frontend forgets it and offers to reopen.

Errors from `session_open` and `connection_test` carry `detail.stage`: `ssh_connect`, `host_key`, `ssh_auth`, `forward`, `db_connect` or `db_auth`.
- `confirmation_required`: show the reason from `detail`; on accept retry `query_execute` with `confirmed: true`.

## Commands

| Command | Arguments | Returns | Phase |
|---|---|---|---|
| `driver_list` | none | `DriverInfo[]` (kind, label, capabilities, option fields) | 1 |
| `connection_list` | none | `ConnectionProfile[]` (no secrets), in the user's order; `folder` groups them in the sidebar | 1 |
| `connection_save` | `{ profile, secrets? }` | `ConnectionProfile` | 1 (`secrets` from 2) |
| `connection_delete` | `{ connectionId }` | none | 1 |
| `connection_duplicate` | `{ connectionId }` | `ConnectionProfile`: a copy named "<name> copy", listed after the original, with its own copy of the keychain secrets | 8 |
| `connection_reorder` | `{ order }`: every connection id once | `ConnectionProfile[]` in the new order; a partial or stale list is `invalid_argument` | 8 |
| `connection_test` | `{ profile, secrets? }` | `{ serverVersion, latencyMs }` | 1 (`secrets` from 2) |
| `session_open` | `{ connectionId }` | `SessionInfo` (sessionId, connectionId, driver, capabilities, readOnly, defaultNamespace) | 1 |
| `session_close` | `{ sessionId }` | none | 1 |
| `sql_split` | `{ sessionId, text }` | `StatementRange[]` (UTF-16 start/end, and `danger`: why it needs confirmation, or null) | 1 |
| `query_execute` | `{ sessionId, tabId, sql, rowLimit (1..=100000), confirmed? }` | `QueryResult` | 1 |
| `query_cancel` | `{ sessionId, tabId }` | none (no-op when the tab is idle) | 2 |
| `tab_close` | `{ sessionId, tabId }` | none | 1 |
| `ssh_trust_host` | `{ host, port, algorithm, fingerprint }` | none (refused if a different key is trusted for that host, port and algorithm) | 3 |
| `ssh_known_hosts_list` | none | `KnownHost[]` | 3 |
| `ssh_known_host_remove` | `{ host, port, algorithm }` | none | 3 |
| `schema_namespaces` | `{ sessionId }` | `Namespace[]` | 4 |
| `schema_objects` | `{ sessionId, namespace }` | `ObjectSummary[]` | 4 |
| `schema_table` | `{ sessionId, table }` | `TableInfo` | 4 |
| `schema_table_ddl` | `{ sessionId, table }` | `string` | 4 |
| `schema_refresh` | `{ sessionId }` | none (clears the session's metadata cache) | 4 |
| `schema_snippets` | `{ sessionId, table }` | `TableSnippets` (`qualifiedName`, `select`), quoted for the dialect | 4 |
| `schema_completion` | `{ sessionId, namespace }` | `CompletionSchema` (cached with the other `schema_*` results) | 5 |
| `history_list` | `{ connectionId, search?, limit (1..=500), before? }` | `HistoryEntry[]`, newest first; `before` is the last id of the previous page | 5 |
| `history_clear` | `{ connectionId }` | none | 5 |
| `table_page` | `{ sessionId, viewId, table, filter?, key?, sort, limit (1..=10000), offset }` | `TablePage` (columns, rows, hasMore, elapsedMs) | 6 |
| `table_count` | `{ sessionId, viewId, table, filter?, key?, exact }` | `RowCount` (`exact`, `estimate` or `unknown`; rows as a string) | 6 |
| `cell_full_value` | `{ sessionId, viewId, table, key, column }` | `Value`, untruncated; `unsupported` without a row key | 6 |
| `changes_preview` | `{ sessionId, viewId, table, operations }` | `string[]`: the statements Apply runs, with literals, in order | 7 |
| `changes_apply` | `{ sessionId, viewId, table, operations }` | `ApplyResult` (statements, rowsAffected); on failure `detail.operation` is the index of the failing operation | 7 |
| `saved_query_list` | none | `SavedQuery[]` (every connection's and the global ones) | 8 |
| `saved_query_save` | `{ query: { id?, connectionId?, name, sql } }` | `SavedQuery`; `connectionId` absent means every connection | 8 |
| `saved_query_delete` | `{ id }` | none | 8 |
| `export_result` | `{ sessionId, columns, rows, format, path }` | `ExportDone` (rowsWritten, path); refuses rows cut short for display | 8 |
| `export_table` | `{ sessionId, viewId, table, filter?, key?, sort, format, path }` plus an `onProgress` channel of `ExportProgress` | `ExportDone`; streams on the view connection `viewId`, cancel with `query_cancel` | 8 |
| `export_text` | `{ sessionId, columns, rows, format }` | `string`: the rows as CSV, JSON or `INSERT`, for copying | 8 |
| `settings_get` | none | `Settings` (theme, editorFontFamily, editorFontSize, defaultRowLimit, defaultPageSize, historyCap, previewPolicy, nullText); defaults until saved | 8 |
| `settings_set` | `{ settings }` | `Settings`; out-of-range values are refused with `invalid_argument` | 8 |
| `logs_dir` | none | `string`: the folder the log files are written to | 8 |

```ts
type QueryResult = {
  outcome: { kind: "rows"; columns: ColumnMeta[]; rows: Value[][]; truncated: boolean }
         | { kind: "affected"; rows: string };
  elapsedMs: number;
  statementClass: "read" | "write" | "ddl" | "transaction" | "session" | "unknown";
};
```

`profile` in `connection_save`/`connection_test` is a `ConnectionDraft`: a profile whose `id` is absent for a new connection. Options are validated against the driver's declared `options`.

## Secrets over IPC

Secrets travel only **from** the frontend to the backend, in `connection_save` and `connection_test` (`secrets: { dbPassword?, sshPassword?, sshKeyPassphrase?, jumpPassword?, jumpKeyPassphrase? }`, each `string | null`). No command returns a secret. `ConnectionProfile` reports only which secrets are stored (`savedSecrets: ["db_password", ...]`). Leaving a secret out of `connection_save` keeps the stored one; `null` deletes it; a string replaces it. `connection_test` uses the given password, else the stored one when the profile has an `id`.

## Tauri permissions

The window gets the minimum capability set: the app's own commands, the dialog plugin (file pickers for SQLite files, keys and exports) and the clipboard plugin. No shell, no HTTP, no general filesystem access from the webview. File paths chosen in dialogs are passed to Rust, which does the reading and writing.

## Long-running calls

A command call is one request and one response. Progress for long operations (exports, multi-statement scripts) is reported through a Tauri `Channel` passed as an argument, when a phase needs it. v1 result sets are bounded by the row limit and returned in one response.

## Events

| Event | Payload | When |
|---|---|---|
| `session_health` | `{ sessionId, status: "ok" \| "unreachable" \| "tunnel_lost" }` | An open session's state changed. Every 60 s the backend pings each session's idle connections (5 s timeout, never a connection with a statement in flight) and drops those that do not answer, so the next use reconnects; the metadata connection is reopened if needed. `unreachable` means no connection could be made; the frontend shows a banner until `ok`. `tunnel_lost` means the backend closed the session, as for the error code. |

## Schema browser

`schema_*` commands run on the session's metadata connection, never on a tab's, so a long statement in an editor tab does not hold up the tree. Results are cached per session, keyed by namespace and table; the cache is cleared by `schema_refresh` and after any statement classified as `ddl` runs in the session (whether or not it succeeded, since MySQL DDL can apply part of its work). The frontend sees `statementClass: "ddl"` in the `QueryResult` (or the error) and reloads the nodes it has open. Errors are per call, so a namespace the user may not read fails on its own node with `permission`, and a table dropped elsewhere fails with `not_found`. A lost metadata connection is reopened on the next call; a lost SSH tunnel closes the session with `tunnel_lost`, as for queries.

## Table data

`table_page`, `table_count` and `cell_full_value` run on the view's own connection, named by `viewId` exactly like an editor tab's `tabId`: a long page can be cancelled with `query_cancel { sessionId, tabId: viewId }`, and closing the view calls `tab_close` with it. They never wait for the schema tree or an editor tab. `key` is a list of `{ column, value }` equalities bound as parameters; `filter` is the user's `WHERE` expression and must be a single expression. Page cells are truncated like query results; `cell_full_value` fetches one whole value by the row's key.

## Edits

`operations` is a list of `{ kind: "insert", values }`, `{ kind: "update", key, set }` and `{ kind: "delete", key }`, each a list of `{ column, value }`. `key` holds the row key's original values and must name exactly the row key's columns. The backend refuses read-only sessions (`read_only`), views, tables without a row key, generated and identity-always columns, and truncated values (`invalid_argument`), whatever the UI allowed. Apply runs deletes, updates, then inserts in one transaction on the view's connection; an `UPDATE` or `DELETE` that does not affect exactly one row fails with `row_count_mismatch`, and everything is rolled back.
