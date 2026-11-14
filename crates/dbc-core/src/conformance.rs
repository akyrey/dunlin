//! Driver conformance suite. Every driver runs these groups from its own tests through a
//! [`Harness`]. Each check returns a [`Failure`] naming what went wrong instead of panicking.

mod changes;
mod introspection;
mod stream;
mod table_data;

use std::sync::Arc;

use async_trait::async_trait;

pub use changes::{
    WriteCase, WriteSupport, apply_changes_group, change_cases, literal_cases,
    literal_round_trip_group,
};
pub use introspection::{IntrospectionSupport, introspection_group};
pub use stream::stream_group;
pub use table_data::{page_sql_cases, table_data_group};

use crate::driver::{ConnectionConfig, Driver};
use crate::error::DbErrorCode;
use crate::result::{ExecuteOptions, StatementOutcome, TypeCategory};
use crate::schema::NamespaceRef;
use crate::value::Value;

pub const SQLITE_FIXTURE: &str = include_str!("../test-support/fixtures/sqlite.sql");
pub const POSTGRES_FIXTURE: &str = include_str!("../test-support/fixtures/postgres.sql");
pub const MYSQL_FIXTURE: &str = include_str!("../test-support/fixtures/mysql.sql");

/// Returns ten rows (`n` = 1..=10) on every supported database.
const TEN_ROWS: &str = "WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM s WHERE n < 10) \
                        SELECT n FROM s";

/// A conformance failure, with enough context to find the broken check.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{check}: {message}")]
pub struct Failure {
    pub check: &'static str,
    pub message: String,
}

fn fail(check: &'static str, message: impl Into<String>) -> Failure {
    Failure {
        check,
        message: message.into(),
    }
}

fn ensure(condition: bool, check: &'static str, message: impl Into<String>) -> Result<(), Failure> {
    if condition {
        Ok(())
    } else {
        Err(fail(check, message))
    }
}

/// One row of a driver's type table: `query` returns one row with one column.
#[derive(Debug, Clone)]
pub struct TypeCase {
    pub query: String,
    pub expected: Value,
    pub category: Option<TypeCategory>,
}

impl TypeCase {
    pub fn new(query: impl Into<String>, expected: Value) -> Self {
        Self {
            query: query.into(),
            expected,
            category: None,
        }
    }

    pub fn with_category(self, category: TypeCategory) -> Self {
        Self {
            category: Some(category),
            ..self
        }
    }
}

/// A statement with a syntax error and the byte offset the server should report, if any.
#[derive(Debug, Clone)]
pub struct SyntaxErrorCase {
    pub sql: String,
    pub position: Option<usize>,
}

/// What a driver provides to run the suite.
#[async_trait]
pub trait Harness: Send + Sync {
    fn driver(&self) -> Arc<dyn Driver>;
    /// A fresh database with the fixture loaded. The harness keeps it alive while it lives.
    async fn database(&self, read_only: bool) -> ConnectionConfig;
    fn unreachable(&self) -> ConnectionConfig;
    /// `None` for databases without authentication.
    fn bad_credentials(&self) -> Option<ConnectionConfig>;
    fn type_cases(&self) -> Vec<TypeCase>;
    fn syntax_error(&self) -> SyntaxErrorCase;
    /// Writes the server itself must refuse on a read-only session.
    fn read_only_rejects(&self) -> Vec<String> {
        [
            "INSERT INTO event_log (message) VALUES ('conformance')",
            "UPDATE orders SET paid = paid WHERE id = 10",
            "DELETE FROM event_log WHERE message IS NULL",
            "CREATE TABLE conformance_t (a integer)",
        ]
        .map(str::to_owned)
        .to_vec()
    }
    /// A statement that runs for many seconds unless cancelled.
    fn slow_statement(&self) -> String;
    /// Where the fixture tables live on a connection made with `config`.
    fn fixture_namespace(&self, config: &ConnectionConfig) -> NamespaceRef;
    /// A namespace `list_namespaces` must report as a system namespace.
    fn system_namespace(&self) -> Option<String> {
        None
    }
    /// Column types and tables for the write groups; `None` skips them.
    fn write_support(&self) -> Option<WriteSupport> {
        None
    }
    fn introspection_support(&self) -> IntrospectionSupport {
        IntrospectionSupport::default()
    }
}

/// Truncated reads repeated back to back, to expose a cancel landing on the next statement.
const TRUNCATE_REPEATS: usize = 50;

/// How long a cancelled statement may take to stop (acceptance: within a second).
const CANCEL_DEADLINE: std::time::Duration = std::time::Duration::from_secs(1);
const CANCEL_RETRY: std::time::Duration = std::time::Duration::from_millis(50);

/// Runs every group, stopping at the first failure.
pub async fn run_all(harness: &dyn Harness) -> Result<(), Failure> {
    connect_group(harness).await?;
    type_round_trip_group(harness).await?;
    row_limit_group(harness).await?;
    execute_group(harness).await?;
    read_only_group(harness).await?;
    cancel_group(harness).await?;
    introspection_group(harness).await?;
    table_data_group(harness).await?;
    literal_round_trip_group(harness).await?;
    apply_changes_group(harness).await?;
    stream_group(harness).await
}

/// Connect, ping, close; unreachable and bad credentials fail with the right codes.
pub async fn connect_group(harness: &dyn Harness) -> Result<(), Failure> {
    let driver = harness.driver();
    let config = harness.database(false).await;
    let connection = driver
        .connect(&config)
        .await
        .map_err(|e| fail("connect", e.to_string()))?;
    connection
        .ping()
        .await
        .map_err(|e| fail("ping", e.to_string()))?;
    let version = connection
        .server_version()
        .await
        .map_err(|e| fail("server_version", e.to_string()))?;
    ensure(
        !version.is_empty(),
        "server_version",
        "empty version string",
    )?;
    connection
        .close()
        .await
        .map_err(|e| fail("close", e.to_string()))?;
    connection
        .close()
        .await
        .map_err(|e| fail("close twice", e.to_string()))?;
    let after_close = connection.ping().await.map_err(|e| e.code);
    ensure(
        after_close == Err(DbErrorCode::ConnectionLost),
        "ping after close",
        format!("expected ConnectionLost, got {after_close:?}"),
    )?;

    let unreachable = driver
        .connect(&harness.unreachable())
        .await
        .err()
        .map(|e| e.code);
    ensure(
        unreachable == Some(DbErrorCode::Connect),
        "unreachable",
        format!("expected Connect, got {unreachable:?}"),
    )?;

    if let Some(bad) = harness.bad_credentials() {
        let code = driver.connect(&bad).await.err().map(|e| e.code);
        ensure(
            code == Some(DbErrorCode::Auth),
            "bad credentials",
            format!("got {code:?}"),
        )?;
    }
    Ok(())
}

/// Every case in the driver's type table decodes to the expected value and category.
pub async fn type_round_trip_group(harness: &dyn Harness) -> Result<(), Failure> {
    let connection = harness
        .driver()
        .connect(&harness.database(false).await)
        .await
        .map_err(|e| fail("connect", e.to_string()))?;
    // Both paths: ad-hoc statements, and prepared statements with parameters, which MySQL
    // answers in its binary protocol.
    for (case, prepared) in harness
        .type_cases()
        .into_iter()
        .flat_map(|case| [(case.clone(), false), (case, true)])
    {
        let options = ExecuteOptions::default();
        let outcome = if prepared {
            connection.query_params(&case.query, &[], &options).await
        } else {
            connection.execute(&case.query, &options).await
        }
        .map_err(|e| {
            fail(
                "type query",
                format!("{} (prepared: {prepared}): {e}", case.query),
            )
        })?;
        let StatementOutcome::Rows(result) = outcome else {
            return Err(fail("type query", format!("{}: no rows", case.query)));
        };
        let actual = result.rows.first().and_then(|row| row.first());
        ensure(
            actual.is_some_and(|v| same_value(v, &case.expected)),
            "type value",
            format!(
                "{} (prepared: {prepared}): expected {:?}, got {actual:?}",
                case.query, case.expected
            ),
        )?;
        if let Some(category) = case.category {
            let actual = result.columns.first().map(|c| c.category);
            ensure(
                actual == Some(category),
                "type category",
                format!("{}: expected {category:?}, got {actual:?}", case.query),
            )?;
        }
    }
    connection
        .close()
        .await
        .map_err(|e| fail("close", e.to_string()))
}

/// NaN never equals itself; everything else compares structurally.
fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) if x.is_nan() && y.is_nan() => true,
        _ => a == b,
    }
}

/// The row limit is honoured and `truncated` is set only when rows were left unread.
pub async fn row_limit_group(harness: &dyn Harness) -> Result<(), Failure> {
    let connection = harness
        .driver()
        .connect(&harness.database(false).await)
        .await
        .map_err(|e| fail("connect", e.to_string()))?;
    for (limit, expected_rows, expected_truncated) in
        [(5, 5, true), (10, 10, false), (20, 10, false)]
    {
        let outcome = connection
            .execute(TEN_ROWS, &ExecuteOptions { row_limit: limit })
            .await
            .map_err(|e| fail("row limit query", e.to_string()))?;
        let StatementOutcome::Rows(result) = outcome else {
            return Err(fail("row limit", "expected rows"));
        };
        ensure(
            result.rows.len() == expected_rows && result.truncated == expected_truncated,
            "row limit",
            format!(
                "limit {limit}: expected {expected_rows} rows (truncated {expected_truncated}), \
                 got {} (truncated {})",
                result.rows.len(),
                result.truncated
            ),
        )?;
    }
    // Stopping early must never leave a cancel behind for the next statement (D-017).
    for _ in 0..TRUNCATE_REPEATS {
        connection
            .execute(TEN_ROWS, &ExecuteOptions { row_limit: 9 })
            .await
            .map_err(|e| fail("row limit repeat", e.to_string()))?;
        let after = connection
            .execute("SELECT 1", &ExecuteOptions::default())
            .await;
        ensure(
            after.is_ok(),
            "row limit",
            format!("statement after a truncated result failed: {after:?}"),
        )?;
    }
    connection
        .close()
        .await
        .map_err(|e| fail("close", e.to_string()))
}

/// Writes report affected rows; syntax errors report `Syntax` and the position.
pub async fn execute_group(harness: &dyn Harness) -> Result<(), Failure> {
    let connection = harness
        .driver()
        .connect(&harness.database(false).await)
        .await
        .map_err(|e| fail("connect", e.to_string()))?;
    let options = ExecuteOptions::default();
    let outcome = connection
        .execute(
            "UPDATE orders SET paid = paid WHERE customer_id = 1",
            &options,
        )
        .await
        .map_err(|e| fail("update", e.to_string()))?;
    ensure(
        outcome == StatementOutcome::Affected { rows: 2 },
        "affected rows",
        format!("expected 2 affected rows, got {outcome:?}"),
    )?;

    let duplicate = connection
        .execute(
            "INSERT INTO customers (id, name) VALUES (1, 'again')",
            &options,
        )
        .await
        .err()
        .map(|e| e.code);
    ensure(
        duplicate == Some(DbErrorCode::Constraint),
        "constraint violation",
        format!("got {duplicate:?}"),
    )?;

    let case = harness.syntax_error();
    let error = connection.execute(&case.sql, &options).await.err();
    let code = error.as_ref().map(|e| e.code);
    ensure(
        code == Some(DbErrorCode::Syntax),
        "syntax error",
        format!("got {code:?}"),
    )?;
    let position = error.and_then(|e| e.position);
    ensure(
        case.position.is_none() || position == case.position,
        "syntax error position",
        format!("expected {:?}, got {position:?}", case.position),
    )?;
    connection
        .close()
        .await
        .map_err(|e| fail("close", e.to_string()))
}

/// A read-only session rejects writes with `ReadOnly` and still serves reads.
pub async fn read_only_group(harness: &dyn Harness) -> Result<(), Failure> {
    let connection = harness
        .driver()
        .connect(&harness.database(true).await)
        .await
        .map_err(|e| fail("connect", e.to_string()))?;
    let options = ExecuteOptions::default();
    for statement in harness.read_only_rejects() {
        let code = connection
            .execute(&statement, &options)
            .await
            .err()
            .map(|e| e.code);
        ensure(
            code == Some(DbErrorCode::ReadOnly),
            "read-only write",
            format!("{statement}: got {code:?}"),
        )?;
    }
    let read = connection.execute("SELECT 1", &options).await;
    ensure(read.is_ok(), "read-only read", format!("{read:?}"))?;
    connection
        .close()
        .await
        .map_err(|e| fail("close", e.to_string()))
}

/// A slow statement is cancelled from another task, reports `Cancelled`, and the connection
/// still works afterwards.
pub async fn cancel_group(harness: &dyn Harness) -> Result<(), Failure> {
    let connection = harness
        .driver()
        .connect(&harness.database(false).await)
        .await
        .map_err(|e| fail("connect", e.to_string()))?;
    let cancel = connection.cancel_handle();
    let statement = harness.slow_statement();
    let started = std::time::Instant::now();
    let options = ExecuteOptions::default();
    let run = connection.execute(&statement, &options);
    let cancel_loop = async {
        // Cancelling before the statement starts may be a no-op, so keep trying.
        loop {
            tokio::time::sleep(CANCEL_RETRY).await;
            if let Err(e) = cancel.cancel().await {
                return e;
            }
        }
    };
    let outcome = tokio::select! {
        outcome = run => outcome,
        error = cancel_loop => return Err(fail("cancel", error.to_string())),
        () = tokio::time::sleep(CANCEL_DEADLINE * 5) => {
            return Err(fail("cancel", "the statement was not cancelled"));
        }
    };
    let elapsed = started.elapsed();
    let code = outcome.err().map(|e| e.code);
    ensure(
        code == Some(DbErrorCode::Cancelled),
        "cancel",
        format!("got {code:?}"),
    )?;
    ensure(
        elapsed < CANCEL_DEADLINE,
        "cancel",
        format!("took {elapsed:?}"),
    )?;
    let after = connection
        .execute("SELECT 1", &ExecuteOptions::default())
        .await;
    ensure(
        after.is_ok(),
        "cancel",
        format!("connection unusable afterwards: {after:?}"),
    )?;
    connection
        .close()
        .await
        .map_err(|e| fail("close", e.to_string()))
}
