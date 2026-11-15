//! Change conformance (Phase 7): statements every dialect snapshots, literals that must mean
//! what the bound value does, and the apply group run on a real database.

use crate::changes::{self, ChangeSet, ColumnType, RowOperation};
use crate::data::{self, TablePageRequest};
use crate::dialect::Dialect;
use crate::driver::Connection;
use crate::error::DbErrorCode;
use crate::result::{ExecuteOptions, StatementOutcome};
use crate::schema::{NamespaceRef, TableInfo, TableRef};
use crate::value::Value;

use super::{Failure, Harness, ensure, fail};

fn kv(name: &str, value: Value) -> (String, Value) {
    (name.to_owned(), value)
}

/// A change set every dialect snapshots (task 7.1): single and composite keys, a key column
/// update, `NULL` in a set and in a key, awkward identifiers, quotes and backslashes.
pub fn change_cases(namespace: &NamespaceRef, types: &[(&str, &str)]) -> ChangeSet {
    let type_of = |name: &str| {
        types
            .iter()
            .find(|(n, _)| *n == name)
            .map_or("", |(_, t)| t)
            .to_owned()
    };
    let columns = ["id", "line", "select", "spaced \"name\"", "note"]
        .into_iter()
        .map(|name| ColumnType {
            name: name.to_owned(),
            type_name: type_of(name),
        })
        .collect();
    ChangeSet {
        table: TableRef {
            namespace: namespace.clone(),
            name: "Order Items".into(),
        },
        columns,
        operations: vec![
            RowOperation::Insert {
                values: vec![
                    kv("id", Value::Int(1)),
                    kv("note", Value::Text("it's a \\ backslash".into())),
                ],
            },
            RowOperation::Insert { values: Vec::new() },
            RowOperation::Update {
                key: vec![kv("id", Value::Int(10)), kv("line", Value::Int(2))],
                set: vec![
                    kv("line", Value::Int(3)),
                    kv("spaced \"name\"", Value::Null),
                ],
            },
            RowOperation::Delete {
                key: vec![kv("id", Value::Int(11)), kv("select", Value::Null)],
            },
        ],
    }
}

/// Every value variant, for the literal snapshot of each dialect (task 7.1).
pub fn literal_cases() -> Vec<(&'static str, Value, &'static str)> {
    vec![
        ("null", Value::Null, "text"),
        ("true", Value::Bool(true), "boolean"),
        ("int", Value::Int(-9_223_372_036_854_775_808), "bigint"),
        ("uint", Value::UInt(u64::MAX), "bigint unsigned"),
        ("float", Value::Float(1.5), "double precision"),
        ("nan", Value::Float(f64::NAN), "double precision"),
        (
            "decimal",
            Value::Decimal("12345678901234567890.123456789".into()),
            "numeric",
        ),
        (
            "decimal_not_a_number",
            Value::Decimal("1; DROP TABLE x".into()),
            "numeric",
        ),
        (
            "text_quotes",
            Value::Text("'; DROP TABLE x; --".into()),
            "text",
        ),
        ("text_backslash", Value::Text("C:\\path\\n".into()), "text"),
        ("text_newline", Value::Text("two\nlines".into()), "text"),
        ("bytes", Value::Bytes(vec![0xde, 0xad, 0x00]), "bytea"),
        ("json", Value::Json("{\"a\": \"it's\"}".into()), "jsonb"),
        ("xml", Value::Xml("<a>b</a>".into()), "xml"),
        (
            "uuid",
            Value::Uuid("6f1c2d3e-0000-4000-8000-000000000001".into()),
            "uuid",
        ),
        ("date", Value::Date("2026-11-14".into()), "date"),
        ("time", Value::Time("10:00:00.5".into()), "time"),
        (
            "timestamp",
            Value::Timestamp("2026-11-14T10:00:00".into()),
            "timestamp",
        ),
        (
            "timestamptz",
            Value::TimestampTz("2026-11-14T10:00:00+01:00".into()),
            "timestamptz",
        ),
        ("interval", Value::Interval("1 day".into()), "interval"),
        (
            "array",
            Value::Array(vec![Value::Int(1), Value::Null]),
            "integer[]",
        ),
        (
            "unknown",
            Value::Unknown {
                type_name: "hstore".into(),
                text: Some("a=>1".into()),
                hex: None,
            },
            "hstore",
        ),
    ]
}

/// A value of one column type: written as a bound parameter and as a literal, it must read
/// back as `read_back` both ways.
#[derive(Debug, Clone)]
pub struct WriteCase {
    pub type_name: &'static str,
    pub value: Value,
    pub read_back: Value,
}

impl WriteCase {
    pub fn same(type_name: &'static str, value: Value) -> Self {
        Self {
            type_name,
            read_back: value.clone(),
            value,
        }
    }
}

/// What a driver provides for the write groups.
#[derive(Debug, Clone)]
pub struct WriteSupport {
    /// The type used for generated strings in the literal round trip.
    pub text_type: &'static str,
    pub integer_type: &'static str,
    pub cases: Vec<WriteCase>,
    /// `CREATE TABLE edits (...)` with `id` (integer primary key), `name` (text, unique),
    /// `a`, `b` (integers) and `ab`, generated as `a + b`.
    pub edits_table: &'static str,
}

/// Characters that break naive quoting, in every script the tests can reach.
const ALPHABET: &[&str] = &[
    "a", "Z", "'", "\"", "\\", "%", "_", ";", "-", "\n", "\t", "é", "中", "😀", " ", "`", "$", "?",
    "--", "/*",
];
const STRINGS: usize = 100;

/// Deterministic pseudo-random strings over `ALPHABET` (a fixed-seed linear congruential
/// generator), so a failure reproduces.
fn strings() -> Vec<String> {
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(state >> 33).unwrap_or(0)
    };
    (0..STRINGS)
        .map(|_| {
            let length = next() % 24;
            (0..length)
                .map(|_| ALPHABET[next() % ALPHABET.len()])
                .collect()
        })
        .collect()
}

async fn exec(connection: &dyn Connection, check: &'static str, sql: &str) -> Result<(), Failure> {
    connection
        .execute(sql, &ExecuteOptions::default())
        .await
        .map(|_| ())
        .map_err(|e| fail(check, format!("{sql}: {e}")))
}

async fn rows(
    connection: &dyn Connection,
    check: &'static str,
    sql: &str,
) -> Result<Vec<Vec<Value>>, Failure> {
    match connection.execute(sql, &ExecuteOptions::default()).await {
        Ok(StatementOutcome::Rows(result)) => Ok(result.rows),
        Ok(_) => Err(fail(check, format!("{sql}: no rows"))),
        Err(e) => Err(fail(check, format!("{sql}: {e}"))),
    }
}

/// Task 7.2: a literal from `render_literal`, executed, stores what the bound value stores.
/// (The app never executes literals; this proves the preview means what runs.)
pub async fn literal_round_trip_group(harness: &dyn Harness) -> Result<(), Failure> {
    let Some(support) = harness.write_support() else {
        return Ok(());
    };
    let driver = harness.driver();
    let config = harness.database(false).await;
    let connection = driver
        .connect(&config)
        .await
        .map_err(|e| fail("connect", e.to_string()))?;
    let dialect = driver.dialect();
    let mut cases = support.cases.clone();
    cases.extend(
        strings()
            .into_iter()
            .map(|s| WriteCase::same(support.text_type, Value::Text(s))),
    );
    for (index, case) in cases.iter().enumerate() {
        let table = format!("rt_{index}");
        exec(
            connection.as_ref(),
            "literal table",
            &format!(
                "CREATE TABLE {table} (id {} PRIMARY KEY, v {})",
                support.integer_type, case.type_name
            ),
        )
        .await?;
        let insert = format!(
            "INSERT INTO {table} (id, v) VALUES (1, {})",
            dialect.placeholder(1, case.type_name)
        );
        connection
            .query_params(
                &insert,
                std::slice::from_ref(&case.value),
                &ExecuteOptions::default(),
            )
            .await
            .map_err(|e| fail("bound write", format!("{case:?}: {e}")))?;
        let literal = dialect.render_literal(&case.value, case.type_name);
        exec(
            connection.as_ref(),
            "literal write",
            &format!("INSERT INTO {table} (id, v) VALUES (2, {literal})"),
        )
        .await?;
        let read = rows(
            connection.as_ref(),
            "read back",
            &format!("SELECT v FROM {table} ORDER BY id"),
        )
        .await?;
        let values: Vec<&Value> = read.iter().filter_map(|row| row.first()).collect();
        ensure(
            values == [&case.read_back, &case.read_back],
            "literal round trip",
            format!("{case:?} as {literal}: read {values:?}"),
        )?;
    }
    connection
        .close()
        .await
        .map_err(|e| fail("close", e.to_string()))
}

struct Edits<'a> {
    connection: &'a dyn Connection,
    dialect: &'a dyn Dialect,
    info: TableInfo,
}

impl Edits<'_> {
    fn set(&self, operations: Vec<RowOperation>) -> ChangeSet {
        ChangeSet {
            table: self.info.table.clone(),
            // As the app does: generated columns are never written.
            columns: self
                .info
                .columns
                .iter()
                .filter(|c| !c.generated)
                .map(|c| ColumnType {
                    name: c.name.clone(),
                    type_name: c.type_name.clone(),
                })
                .collect(),
            operations,
        }
    }

    async fn apply(&self, operations: Vec<RowOperation>) -> Result<(), changes::ApplyFailure> {
        changes::apply(self.connection, self.dialect, &self.set(operations))
            .await
            .map(|_| ())
    }

    /// `(id, name, ab)` of every row, by id.
    async fn state(&self) -> Result<Vec<(Value, Value, Value)>, Failure> {
        let request = TablePageRequest {
            table: self.info.table.clone(),
            filter: None,
            key: Vec::new(),
            sort: Vec::new(),
            limit: 100,
            offset: 0,
            after: Vec::new(),
        };
        let page = data::fetch_page(self.connection, self.dialect, &self.info, &request)
            .await
            .map_err(|e| fail("read edits", e.to_string()))?;
        let index = |name: &str| {
            self.info
                .columns
                .iter()
                .position(|c| c.name == name)
                .unwrap_or(0)
        };
        let (id, name, ab) = (index("id"), index("name"), index("ab"));
        Ok(page
            .rows
            .into_iter()
            .map(|row| {
                let get = |i: usize| row.get(i).cloned().unwrap_or(Value::Null);
                (get(id), get(name), get(ab))
            })
            .collect())
    }
}

fn text(s: &str) -> Value {
    Value::Text(s.to_owned())
}

fn row(id: i64, name: Option<&str>, ab: i64) -> (Value, Value, Value) {
    (
        Value::Int(id),
        name.map_or(Value::Null, text),
        Value::Int(ab),
    )
}

fn insert(id: i64, name: &str) -> RowOperation {
    RowOperation::Insert {
        values: vec![
            kv("id", Value::Int(id)),
            kv("name", text(name)),
            kv("a", Value::Int(id)),
            kv("b", Value::Int(1)),
        ],
    }
}

fn by_id(id: i64) -> Vec<(String, Value)> {
    vec![kv("id", Value::Int(id))]
}

/// Task 7.4: batches apply in one transaction, in order, and roll back completely.
pub async fn apply_changes_group(harness: &dyn Harness) -> Result<(), Failure> {
    let Some(support) = harness.write_support() else {
        return Ok(());
    };
    let driver = harness.driver();
    let config = harness.database(false).await;
    let connection = driver
        .connect(&config)
        .await
        .map_err(|e| fail("connect", e.to_string()))?;
    let dialect = driver.dialect();
    exec(connection.as_ref(), "edits table", support.edits_table).await?;
    let table = TableRef {
        namespace: harness.fixture_namespace(&config),
        name: "edits".into(),
    };
    let info = connection
        .describe_table(&table)
        .await
        .map_err(|e| fail("describe edits", e.to_string()))?;
    let edits = Edits {
        connection: connection.as_ref(),
        dialect: dialect.as_ref(),
        info,
    };
    let applied = |check: &'static str| {
        move |e: changes::ApplyFailure| fail(check, format!("{e} (operation {:?})", e.operation))
    };

    edits
        .apply(vec![insert(1, "one"), insert(2, "two"), insert(3, "three")])
        .await
        .map_err(applied("insert"))?;
    edits
        .apply(vec![
            insert(4, "four"),
            RowOperation::Update {
                key: by_id(2),
                set: vec![kv("name", text("'; DROP TABLE x; --"))],
            },
            RowOperation::Delete { key: by_id(3) },
        ])
        .await
        .map_err(applied("mixed batch"))?;
    edits
        .apply(vec![RowOperation::Update {
            key: by_id(4),
            set: vec![kv("id", Value::Int(5)), kv("name", Value::Null)],
        }])
        .await
        .map_err(applied("key update"))?;
    let expected = vec![
        row(1, Some("one"), 2),
        row(2, Some("'; DROP TABLE x; --"), 3),
        row(5, None, 5),
    ];
    let state = edits.state().await?;
    ensure(
        state == expected,
        "applied state",
        format!("expected {expected:?}, got {state:?}"),
    )?;

    // A constraint violation in the last statement undoes the whole batch.
    let duplicate = edits
        .apply(vec![
            RowOperation::Update {
                key: by_id(1),
                set: vec![kv("name", text("changed"))],
            },
            insert(2, "duplicate key"),
        ])
        .await
        .err();
    ensure(
        duplicate
            .as_ref()
            .is_some_and(|f| f.operation == Some(1) && f.error.code == DbErrorCode::Constraint),
        "constraint rollback",
        format!("got {duplicate:?}"),
    )?;
    // A row deleted elsewhere is a row count mismatch, and nothing else is applied.
    let gone = edits
        .apply(vec![
            RowOperation::Update {
                key: by_id(1),
                set: vec![kv("name", text("changed"))],
            },
            RowOperation::Delete { key: by_id(99) },
        ])
        .await
        .err();
    ensure(
        gone.as_ref().is_some_and(|f| {
            f.operation == Some(1) && f.error.code == DbErrorCode::RowCountMismatch
        }),
        "row count rollback",
        format!("got {gone:?}"),
    )?;
    let after = edits.state().await?;
    ensure(
        after == expected,
        "rolled back",
        format!("expected {expected:?}, got {after:?}"),
    )?;

    connection
        .close()
        .await
        .map_err(|e| fail("close", e.to_string()))
}
