# Driver API

Everything here lives in `dbc-core`. The Rust below is the intended shape, not final code: names and signatures may be refined during Phase 1, and this document must be updated when they are.

## Traits

```rust
pub trait Driver: Send + Sync {
    fn kind(&self) -> DriverKind;                 // Postgres | MySql | Sqlite | ...
    fn capabilities(&self) -> Capabilities;
    fn dialect(&self) -> &dyn Dialect;
    async fn connect(&self, config: &ConnectionConfig) -> Result<Box<dyn Connection>, DbError>;
}

pub trait Connection: Send + Sync {
    async fn ping(&self) -> Result<(), DbError>;

    // Ad-hoc execution of exactly one statement.
    async fn execute(&self, sql: &str, options: &ExecuteOptions) -> Result<StatementOutcome, DbError>;
    fn cancel_handle(&self) -> Arc<dyn CancelHandle>;

    // Introspection.
    async fn list_namespaces(&self) -> Result<Vec<Namespace>, DbError>;
    async fn list_objects(&self, namespace: &NamespaceRef) -> Result<Vec<ObjectSummary>, DbError>;
    async fn describe_table(&self, table: &TableRef) -> Result<TableInfo, DbError>;
    async fn table_ddl(&self, table: &TableRef) -> Result<String, DbError>;
    async fn completion_schema(&self, namespace: &NamespaceRef) -> Result<CompletionSchema, DbError>;

    // Table data.
    async fn fetch_table_page(&self, request: &TablePageRequest) -> Result<ResultSet, DbError>;
    async fn count_rows(&self, table: &TableRef, filter: Option<&str>, exact: bool) -> Result<RowCount, DbError>;
    async fn apply_changes(&self, changes: &ChangeSet) -> Result<ApplyReport, DbError>;

    async fn close(self: Box<Self>) -> Result<(), DbError>;
}

pub trait CancelHandle: Send + Sync {
    async fn cancel(&self) -> Result<(), DbError>;
}
```

Async trait methods use `async-trait` (or native async-in-trait with boxed futures) so that `Box<dyn Connection>` works. Methods take `&self`; a driver whose client needs `&mut` wraps it in a `tokio::sync::Mutex`.

## Connection configuration

```rust
pub struct ConnectionConfig {
    pub endpoint: Endpoint,          // Tcp { dial_host, dial_port, server_name } | File { path }
    pub database: Option<String>,
    pub user: Option<String>,
    pub password: Option<SecretString>,
    pub tls: TlsConfig,              // mode: Disable | Prefer | Require | VerifyCa | VerifyFull; ca, client cert/key
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
    pub explain: bool,
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

Tagged, with string payloads:

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

pub struct ObjectSummary { pub name: String, pub kind: ObjectKind /* Table | View | MaterializedView */, pub comment: Option<String> }

pub struct TableInfo {
    pub table: TableRef,
    pub kind: ObjectKind,
    pub columns: Vec<Column>,            // name, type_name, category, nullable, default, comment, ordinal, generated
    pub primary_key: Option<Key>,        // name, ordered column names
    pub indexes: Vec<Index>,             // name, columns, unique, method, predicate
    pub foreign_keys: Vec<ForeignKey>,   // name, columns, referenced table + columns, on_update, on_delete
    pub unique_constraints: Vec<Key>,
    pub check_constraints: Vec<Check>,   // name, expression
    pub row_key: Option<Vec<String>>,    // columns used to address a row for editing; None = read-only
}
```

`row_key` is the primary key, or else the first unique constraint whose columns are all `NOT NULL`.

How the three databases map onto namespaces:

| Database | catalog | schema | v1 tree |
|---|---|---|---|
| PostgreSQL | connected database | schema | schemas of the connected database only |
| MySQL | (none) | database | all databases the user can see |
| SQLite | (none) | `main` / attached name | `main` and attached databases |

`completion_schema` returns schema → table → column names for one namespace in a single query, to feed editor autocomplete.

## Dialect

```rust
pub trait Dialect: Send + Sync {
    fn quote_identifier(&self, name: &str) -> String;            // doubles embedded quotes
    fn qualified_name(&self, table: &TableRef) -> String;
    fn select_page(&self, request: &TablePageRequest) -> String;
    fn placeholder(&self, index: usize, type_name: &str) -> String;   // "$1::text::int4", "?", "?1"
    fn render_literal(&self, value: &Value, type_name: &str) -> String; // preview only, never executed
    fn splitter_rules(&self) -> SplitterRules;                   // dollar quoting, backticks, comment styles
}
```

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
```

`dbc-core::changes` turns a `ChangeSet` plus a `Dialect` into `Vec<BoundStatement { sql, params }>` (for execution) and `Vec<String>` (for preview). This is pure and heavily unit-tested. The driver's `apply_changes`:

1. begins a transaction;
2. runs deletes, then updates, then inserts;
3. checks that every `UPDATE` and `DELETE` affected exactly one row;
4. commits, or rolls back on any error or count mismatch and returns which operation failed.

`key` always holds the **original** values of the row key, so editing a key column works.

## Statement utilities

- `sql::split(text, rules) -> Vec<StatementRange { start, end }>` splits on `;` outside strings, quoted identifiers, comments and PostgreSQL dollar-quoted bodies. MySQL's `DELIMITER` command is not supported in v1.
- `sql::classify(statement) -> StatementClass` returns `Read`, `Write`, `Ddl`, `Transaction`, `Session` or `Unknown`, plus a `dangerous` flag for `UPDATE`/`DELETE` without `WHERE`, `DROP`, `TRUNCATE`. It is a lexer-level heuristic: `Unknown` is treated as a write on read-only sessions.

## Errors

```rust
pub struct DbError {
    pub code: DbErrorCode,       // Connect | Auth | Tls | Timeout | Cancelled | Syntax | Constraint | Permission
                                 // | ConnectionLost | Unsupported | RowCountMismatch | Internal
    pub message: String,
    pub detail: Option<String>,  // server detail/hint
    pub position: Option<usize>, // character offset in the statement, when the server reports one
    pub sqlstate: Option<String>,
}
```

## Per-driver notes

### PostgreSQL (`tokio-postgres`)

- **Decoding.** The extended protocol returns binary values and `tokio-postgres` only decodes types it knows. Plan: a `RawValue` type implementing `FromSql` that accepts every type and keeps the raw bytes, then our own decoding by type OID and `Kind` (enum labels and domains decode as their base representation; arrays recurse). Types we cannot decode become `Value::Unknown` with hex. **Verify in Phase 2** with a spike covering `numeric`, `jsonb`, `timestamptz`, `interval`, enum, domain, array, `uuid`, `bytea`, `inet`, range. Fallback if this proves too costly: the simple query protocol, which returns everything as text.
- **Editing.** Parameters are bound as text and cast in SQL (`$1::text::int4`), so the driver does not need a typed encoder per column type.
- **Read-only.** `SET default_transaction_read_only = on` after connecting.
- **DDL.** PostgreSQL has no `SHOW CREATE TABLE`; DDL is reconstructed from the catalogs and labelled as reconstructed.
- **TLS.** `rustls` via `tokio-postgres-rustls`.

### MySQL / MariaDB (`mysql_async`)

- Text protocol for ad-hoc queries gives values as text plus column type metadata; map by column type and flags (unsigned, binary).
- `TINYINT(1)` is shown as an integer, not a boolean.
- Read-only: `SET SESSION TRANSACTION READ ONLY`.
- DDL: `SHOW CREATE TABLE`.
- Cancellation needs the connection id (`CONNECTION_ID()`), captured at connect time.

### SQLite (`rusqlite`, bundled)

- Values are dynamically typed per cell; the declared column type gives the category, the stored value gives the `Value` variant.
- Read-only: open with `SQLITE_OPEN_READ_ONLY`.
- DDL: `sqlite_master.sql`. Introspection through `PRAGMA table_info`, `index_list`, `index_info`, `foreign_key_list`.
- `row_key` falls back to the primary key only; `rowid` addressing is deferred.
- `PRAGMA foreign_keys = ON` is set per connection.
