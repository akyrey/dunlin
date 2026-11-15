# Driver API

Everything here lives in `dbc-core`. The Rust below is the intended shape, not final code: names and signatures may be refined during Phase 1, and this document must be updated when they are.

## Traits

```rust
#[async_trait]
pub trait Driver: Send + Sync {
    fn info(&self) -> DriverInfo;                 // kind, label, endpoint kind, capabilities, option fields
    fn dialect(&self) -> Arc<dyn Dialect>;
    async fn connect(&self, config: &ConnectionConfig) -> Result<Arc<dyn Connection>, DbError>;
}

#[async_trait]
pub trait Connection: Send + Sync {
    async fn ping(&self) -> Result<(), DbError>;
    async fn server_version(&self) -> Result<String, DbError>;

    // Ad-hoc execution of exactly one statement.
    async fn execute(&self, sql: &str, options: &ExecuteOptions) -> Result<StatementOutcome, DbError>;
    fn cancel_handle(&self) -> Arc<dyn CancelHandle>;

    // Introspection (Phase 4). Default implementations return `Unsupported`.
    async fn list_namespaces(&self) -> Result<Vec<Namespace>, DbError>;
    async fn list_objects(&self, namespace: &NamespaceRef) -> Result<Vec<ObjectSummary>, DbError>;
    async fn describe_table(&self, table: &TableRef) -> Result<TableInfo, DbError>;
    async fn table_ddl(&self, table: &TableRef) -> Result<String, DbError>;
    async fn completion_schema(&self, namespace: &NamespaceRef) -> Result<CompletionSchema, DbError>;

    async fn default_namespace(&self) -> Result<Option<NamespaceRef>, DbError>;   // default Ok(None)

    // Table data (Phase 6). Pages, counts and cells are built in `dbc_core::data` from the
    // dialect and run through `query_params` (D-022).
    async fn query_params(&self, sql: &str, params: &[Value], options: &ExecuteOptions) -> Result<StatementOutcome, DbError>;
    async fn estimate_rows(&self, table: &TableRef) -> Result<Option<u64>, DbError>;   // default Ok(None)
    // Exports (Phase 8): rows in batches over a bounded channel; a dropped receiver cancels.
    async fn stream_rows(&self, sql: &str, params: &[Value], rows: mpsc::Sender<StreamEvent>) -> Result<u64, DbError>;

    // Edits (Phase 7) need no method of their own: `dbc_core::changes::apply` runs them through
    // `execute` (BEGIN/COMMIT/ROLLBACK) and `query_params` (D-023).

    // Idempotent. Later calls on the connection fail with `ConnectionLost`.
    async fn close(&self) -> Result<(), DbError>;
}

#[async_trait]
pub trait CancelHandle: Send + Sync {
    async fn cancel(&self) -> Result<(), DbError>;
}

pub struct DriverInfo {
    pub kind: DriverKind,
    pub label: String,
    pub endpoint: EndpointKind,          // Tcp | File: decides the connection form fields
    pub default_port: Option<u16>,       // pre-filled for Tcp drivers
    pub tls_modes: Vec<TlsMode>,         // offered on the form; empty = no TLS (D-018)
    pub capabilities: Capabilities,
    pub options: Vec<OptionField>,       // key, label, kind (Bool | Text), default, help
}
```

Traits use `async-trait` (D-013). Connections are shared as `Arc<dyn Connection>`, so `close` takes `&self`; this lets a running statement and its cancellation, or a close, happen from different tasks. Methods take `&self`; a driver whose client needs `&mut` wraps it in a `tokio::sync::Mutex` (or, for SQLite, a worker thread).

## Connection configuration

```rust
pub struct ConnectionConfig {
    pub endpoint: Endpoint,          // Tcp { dial_host, dial_port, server_name } | File { path }
    pub database: Option<String>,
    pub user: Option<String>,
    pub password: Option<SecretString>,
    pub tls: TlsConfig,              // mode: Disable | Prefer | Require | VerifyCa | VerifyFull; ca_file, client_cert_file, client_key_file
    pub read_only: bool,
    pub options: BTreeMap<String, String>,   // driver-specific extras
}
```

`dial_host`/`dial_port` is where the socket connects (the tunnel's loopback port when tunnelling); `server_name` is the real host, used for TLS verification. `SecretString` zeroises on drop and has a redacted `Debug`.

## Capabilities

```rust
pub struct Capabilities {
    pub schemas: bool,               // has a schema level under the database (PostgreSQL)
    pub multiple_databases: bool,    // can list other databases from one connection (MySQL)
    pub cancel: bool,
    pub read_only_session: bool,     // server-enforced read-only
    pub transactional_ddl: bool,
    pub explain: bool,  // Connection::explain(statement) -> PlanNode, see dbc_core::plan
    pub tls: bool,
    pub tunnel: bool,                // false for file databases
}
```

The frontend hides or disables features from these flags. It must never branch on `DriverKind`.

## Value model

```rust
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),                 // MySQL BIGINT UNSIGNED
    Float(f64),
    Decimal(String),           // exact text from the server
    Text(String),
    Bytes(Vec<u8>),
    Json(String),              // original text, not re-serialised
    Xml(String),
    Uuid(String),
    Date(String),              // ISO 8601
    Time(String),
    Timestamp(String),         // no zone
    TimestampTz(String),       // with offset
    Interval(String),
    Array(Vec<Value>),
    Unknown { type_name: String, text: Option<String>, hex: Option<String> },
}
```

### Wire format

Tagged, with string payloads. The wire form is its own type, `WireValue` (generated for TypeScript as `Value`); `Value::into_wire(limit)` converts and truncates, and `Value`'s `Serialize`/`Deserialize` go through it without truncation. Deserialising a value flagged `truncated` fails, so a truncated display value can never be written back.

```json
{"t":"null"}
{"t":"bool","v":true}
{"t":"int","v":"9007199254740993"}
{"t":"float","v":"NaN"}
{"t":"decimal","v":"12345678901234567890.123456789"}
{"t":"text","v":"hello"}
{"t":"bytes","v":"3q2+7w==","len":4}
{"t":"json","v":"{\"a\": 1}"}
{"t":"array","v":[{"t":"int","v":"1"},{"t":"null"}]}
{"t":"unknown","type":"geometry","hex":"0101..."}
```

Rules:

- Numbers are always strings on the wire. Only `bool` and `array` use a non-string payload.
- `bytes` is base64 and is truncated for display beyond a size threshold (default 64 KiB), with `len` giving the true size. The full value is fetched on demand by the cell viewer.
- Long `text`/`json`/`xml` values are truncated the same way, flagged with `"truncated": true`.

### Column metadata

```rust
pub struct ColumnMeta {
    pub name: String,
    pub type_name: String,           // as the database calls it: "int8", "varchar(255)", "JSONB"
    pub category: TypeCategory,      // Bool | Integer | Float | Decimal | Text | Json | Xml | Binary | Temporal | Uuid | Array | Other
    pub nullable: Option<bool>,      // unknown for arbitrary expressions
}
```

`category` is what the frontend uses to choose alignment, the cell editor and the cell viewer.

## Results

```rust
pub enum StatementOutcome {
    Rows(ResultSet),
    Affected { rows: u64 },
}

pub struct ResultSet {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Value>>,
    pub truncated: bool,
}

pub struct ExecuteOptions {
    pub row_limit: usize,            // default 1000
}
```

## Schema model

```rust
pub struct NamespaceRef { pub catalog: Option<String>, pub schema: Option<String> }
pub struct TableRef     { pub namespace: NamespaceRef, pub name: String }

pub struct Namespace     { pub namespace: NamespaceRef, pub name: String, pub system: bool }
pub struct ObjectSummary { pub name: String, pub kind: ObjectKind /* Table | View | MaterializedView */, pub comment: Option<String> }

pub struct TableInfo {
    pub table: TableRef,
    pub kind: ObjectKind,
    pub comment: Option<String>,
    pub columns: Vec<Column>,            // name, type_name, category, nullable, default, comment, ordinal, generated
    pub primary_key: Option<Key>,        // name, ordered column names
    pub indexes: Vec<Index>,             // name, columns, unique, method, predicate
    pub foreign_keys: Vec<ForeignKey>,   // name, columns, referenced table + columns, on_update, on_delete
    pub unique_constraints: Vec<Key>,
    pub check_constraints: Vec<Check>,   // name, expression
    pub row_key: Option<Vec<String>>,    // columns used to address a row for editing; None = read-only
}
```

`row_key` is the primary key, else the first unique constraint whose columns are all `NOT NULL`, else the first such unique index without a predicate (`TableInfo::choose_row_key`, shared by every driver).

Rules every driver follows (checked by the introspection conformance group, D-020):

- `system` marks catalog namespaces (`pg_catalog`, `information_schema`, `mysql`, SQLite's `temp`); the tree hides them unless "show system objects" is on.
- `indexes` lists only indexes that do not back the primary key or a unique constraint; those appear once, as keys.
- An expression key part is reported as its expression text in `Index::columns`.
- A generated column has `generated = true` and its expression in `default`. An identity column's `default` reads `GENERATED ALWAYS AS IDENTITY` (or `BY DEFAULT`).
- `on_update`/`on_delete` are `None` for `NO ACTION`. MySQL's `RESTRICT` is reported as `None` too: InnoDB treats them alike and the servers disagree on which one they report.
- A table, view or namespace that does not exist is `DbErrorCode::NotFound`; one the user may not read is `Permission`.
- Names the database invents are reported as it reports them (`orders_total_check`, `employees_ibfk_1`); SQLite does not name constraints, so its names are `None` unless the `CREATE TABLE` text names them.
- `table_ddl` is the server's own text where one exists (MySQL `SHOW CREATE`, SQLite `sqlite_master.sql` plus its indexes and triggers). PostgreSQL has none, so its DDL is rebuilt from the catalogs and starts with a comment saying so.

How the three databases map onto namespaces:

| Database | catalog | schema | v1 tree |
|---|---|---|---|
| PostgreSQL | (none: always the connected database) | schema | schemas of the connected database only |
| MySQL | (none) | database | all databases the user can see |
| SQLite | (none) | `main` / attached name | `main` and attached databases |

`completion_schema` returns schema → table → column names for one namespace in a single query, to feed editor autocomplete.

## Dialect

```rust
pub trait Dialect: Send + Sync {
    fn quote_identifier(&self, name: &str) -> String;            // doubles embedded quotes (helper: quote_with)
    fn qualified_name(&self, table: &TableRef) -> String;        // default: quoted parts joined by "."
    fn splitter_rules(&self) -> SplitterRules;                   // dollar quoting, backticks, comment styles

    fn placeholder(&self, index: usize, type_name: &str) -> String;   // "$1::text::integer", "?", "?1"

    fn render_literal(&self, value: &Value, type_name: &str) -> String;  // preview only, never executed (default: ANSI)
    fn default_values(&self) -> &'static str;                             // "DEFAULT VALUES"; MySQL "() VALUES ()"
}
```

Table pages come from `dbc_core::data::page_sql(dialect, info, request)`: `SELECT * FROM <qualified> [WHERE (<filter>) AND <key> = <placeholder>...] [ORDER BY <sort, else row key>] LIMIT n OFFSET m`. Sort and key columns must be columns of the table; the filter must be one expression. `count_sql` and `cell_sql` follow the same rules.

`render_literal` exists only to show the user readable SQL in the change preview and in `INSERT` export. Executed statements always bind parameters.

## Change sets

```rust
pub struct ChangeSet {
    pub table: TableRef,
    pub columns: Vec<ColumnType>,                 // name + type_name, for placeholders
    pub operations: Vec<RowOperation>,
}

pub enum RowOperation {
    Insert { values: Vec<(String, Value)> },                        // omitted columns take their default
    Update { key: Vec<(String, Value)>, set: Vec<(String, Value)> },
    Delete { key: Vec<(String, Value)> },
}

pub struct ApplyReport { pub statements: usize, pub rows_affected: u64 }
pub struct ApplyFailure { pub operation: Option<usize>, pub error: DbError }   // index into operations
```

`dbc_core::changes::plan` turns a `ChangeSet` plus a `Dialect` into `PlannedStatement`s, each a list of SQL text and typed values. `bound()` renders it with placeholders and parameters (executed), `preview()` with `render_literal` (shown), so both come from one structure. This is pure and heavily unit-tested. `dbc_core::changes::apply`:

1. begins a transaction;
2. runs deletes, then updates, then inserts;
3. checks that every `UPDATE` and `DELETE` affected exactly one row;
4. commits, or rolls back on any error or count mismatch and returns which operation failed.

`key` always holds the **original** values of the row key, so editing a key column works.

## Statement utilities

- `sql::split(text, rules) -> Vec<StatementRange { start, end }>` splits on `;` outside strings, quoted identifiers, comments and PostgreSQL dollar-quoted bodies. Inside a `CREATE` statement, `BEGIN ... END` blocks (SQLite triggers, SQL-standard function bodies) do not split. Ranges are **byte** offsets, exclude surrounding whitespace and the `;`, and segments containing only comments are dropped. The IPC layer converts offsets to UTF-16 (`sql::byte_to_utf16`). MySQL's `DELIMITER` command is not supported in v1.
- `sql::classify(statement, rules) -> Classification { class, danger }`. `class` is `Read`, `Write`, `Ddl`, `Transaction`, `Session` or `Unknown`; `danger` is `UpdateWithoutWhere`, `DeleteWithoutWhere`, `Drop` or `Truncate` (with a user-facing `reason()`). It is a lexer-level heuristic: `Unknown` is treated as a write on read-only sessions (`StatementClass::allowed_on_read_only`). Refinements: `SELECT ... INTO` and data-modifying CTEs are `Write`; `EXPLAIN ANALYZE` is classified as the statement it runs; `SET`/`BEGIN`/`RESET` statements that would undo the server-side read-only guard (`READ WRITE`, anything naming `read_only`, `RESET ALL`) are `Write`.

## Errors

```rust
pub struct DbError {
    pub code: DbErrorCode,       // Connect | Auth | Tls | Timeout | Cancelled | Syntax | Constraint | Permission
                                 // | ReadOnly | ConnectionLost | Unsupported | RowCountMismatch | Internal
    pub message: String,
    pub detail: Option<String>,  // server detail/hint
    pub position: Option<usize>, // byte offset in the statement, when the server reports one (UTF-16 over IPC)
    pub sqlstate: Option<String>,
}
```

## Per-driver notes

### PostgreSQL (`tokio-postgres`)

- **Decoding (D-016, verified).** The extended protocol returns binary values and `tokio-postgres` only decodes types it knows. Plan: a `RawValue` type implementing `FromSql` that accepts every type and keeps the raw bytes, then our own decoding by type OID and `Kind` (enum labels and domains decode as their base representation; arrays recurse). Types we cannot decode become `Value::Unknown` with hex. **Verify in Phase 2** with a spike covering `numeric`, `jsonb`, `timestamptz`, `interval`, enum, domain, array, `uuid`, `bytea`, `inet`, range. Fallback if this proves too costly: the simple query protocol, which returns everything as text.
- **Editing.** Parameters are bound as text and cast in SQL (`$1::text::int4`), so the driver does not need a typed encoder per column type.
- **Read-only.** `SET default_transaction_read_only = on` after connecting.
- **DDL.** PostgreSQL has no `SHOW CREATE TABLE`; DDL is reconstructed from the catalogs and labelled as reconstructed.
- **TLS.** `rustls` via `tokio-postgres-rustls`.

### MySQL / MariaDB (`mysql_async`)

- Text protocol for ad-hoc queries gives values as text plus column type metadata; map by column type and flags (unsigned, binary).
- `TINYINT(1)` is shown as an integer, not a boolean.
- Read-only: `SET SESSION TRANSACTION READ ONLY`. This does not stop DDL (it commits implicitly); the backend classifier rejects DDL on read-only sessions.
- `CLIENT_FOUND_ROWS` is set, so affected rows mean *matched* rows, as in PostgreSQL and SQLite.
- The client always enables multi-statements; the driver refuses text that splits into more than one statement.
- A user-cancelled `SELECT SLEEP()` returns a row instead of an error; such results are reported as `Cancelled`.
- DDL: `SHOW CREATE TABLE`.
- Cancellation needs the connection id (`CONNECTION_ID()`), captured at connect time.

### SQLite (`rusqlite`, bundled)

- Values are dynamically typed per cell; the declared column type gives the category, the stored value gives the `Value` variant.
- Read-only: open with `SQLITE_OPEN_READ_ONLY`.
- DDL: `sqlite_master.sql`. Introspection through `PRAGMA table_info`, `index_list`, `index_info`, `foreign_key_list`.
- `row_key` falls back to the primary key only; `rowid` addressing is deferred.
- `PRAGMA foreign_keys = ON` and a 5 s busy timeout are set per connection.
- The file must exist unless the driver option `create_if_missing` is `true`; read-only connections never create. Opening reads `sqlite_master` so a non-database file fails at connect (`Connect`), not on the first query.
- Affected rows: `changes()` is used only if `total_changes()` moved, because SQLite keeps the last DML count across DDL.
- Error mapping: `SQLITE_ERROR` (syntax, unknown table or column) is `Syntax`, with the byte offset from `sqlite3_error_offset` when available.
