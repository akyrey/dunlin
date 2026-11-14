#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::sync::Arc;
use std::time::Duration;

use dbc_core::driver::{Connection, Driver};
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_core::{DbErrorCode, Value};
use dbc_driver_sqlite::SqliteDriver;

use common::{TempDatabases, config, creating};

const INFINITE_QUERY: &str =
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c";

async fn open_fixture(databases: &TempDatabases) -> Arc<dyn Connection> {
    SqliteDriver
        .connect(&config(databases.fixture(), false))
        .await
        .unwrap()
}

async fn rows(connection: &dyn Connection, sql: &str) -> Vec<Vec<Value>> {
    match connection
        .execute(sql, &ExecuteOptions::default())
        .await
        .unwrap()
    {
        StatementOutcome::Rows(result) => result.rows,
        other => panic!("expected rows, got {other:?}"),
    }
}

#[tokio::test]
async fn missing_file_without_create_option_fails_to_connect() {
    let databases = TempDatabases::default();
    let path = databases.dir().join("absent.db");

    let error = SqliteDriver
        .connect(&config(&path, false))
        .await
        .err()
        .unwrap();

    assert_eq!(error.code, DbErrorCode::Connect);
    assert!(!path.exists());
}

#[tokio::test]
async fn create_option_creates_a_new_database_file() {
    let databases = TempDatabases::default();
    let path = databases.dir().join("new.db");

    let connection = SqliteDriver.connect(&creating(&path)).await.unwrap();
    connection.close().await.unwrap();

    assert!(path.exists());
}

#[tokio::test]
async fn read_only_ignores_create_option() {
    let databases = TempDatabases::default();
    let path = databases.dir().join("absent.db");
    let mut read_only = creating(&path);
    read_only.read_only = true;

    let error = SqliteDriver.connect(&read_only).await.err().unwrap();

    assert_eq!(error.code, DbErrorCode::Connect);
}

#[tokio::test]
async fn a_file_that_is_not_a_database_fails_to_connect() {
    let databases = TempDatabases::default();
    let path = databases.dir().join("notes.txt");
    std::fs::write(
        &path,
        "this is not a database, just some text to fill a page",
    )
    .unwrap();

    let error = SqliteDriver
        .connect(&config(&path, false))
        .await
        .err()
        .unwrap();

    assert_eq!(error.code, DbErrorCode::Connect);
}

#[tokio::test]
async fn foreign_keys_are_enforced() {
    let databases = TempDatabases::default();
    let connection = open_fixture(&databases).await;

    let error = connection
        .execute(
            "INSERT INTO orders (id, customer_id) VALUES (99, 12345)",
            &ExecuteOptions::default(),
        )
        .await
        .err()
        .unwrap();

    assert_eq!(error.code, DbErrorCode::Constraint);
}

#[tokio::test]
async fn ddl_reports_zero_affected_rows_after_dml() {
    let databases = TempDatabases::default();
    let connection = open_fixture(&databases).await;
    let options = ExecuteOptions::default();
    connection
        .execute("DELETE FROM event_log", &options)
        .await
        .unwrap();

    let outcome = connection
        .execute("CREATE TABLE t (a INTEGER)", &options)
        .await
        .unwrap();

    assert_eq!(outcome, StatementOutcome::Affected { rows: 0 });
}

#[tokio::test]
async fn multiple_statements_are_rejected() {
    let databases = TempDatabases::default();
    let connection = open_fixture(&databases).await;

    let error = connection
        .execute("SELECT 1; SELECT 2", &ExecuteOptions::default())
        .await
        .err()
        .unwrap();

    assert_eq!(error.code, DbErrorCode::Syntax);
}

#[tokio::test]
async fn insert_returning_produces_rows() {
    let databases = TempDatabases::default();
    let connection = open_fixture(&databases).await;

    let result = rows(
        &*connection,
        "INSERT INTO event_log (message) VALUES ('x') RETURNING message",
    )
    .await;

    assert_eq!(result, vec![vec![Value::Text("x".into())]]);
}

#[tokio::test]
async fn columns_carry_names_and_declared_types() {
    let databases = TempDatabases::default();
    let connection = open_fixture(&databases).await;

    let outcome = connection
        .execute(
            "SELECT id, total, 1 + 1 AS two FROM orders",
            &ExecuteOptions::default(),
        )
        .await
        .unwrap();

    let StatementOutcome::Rows(result) = outcome else {
        panic!("expected rows")
    };
    let columns: Vec<(&str, &str)> = result
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c.type_name.as_str()))
        .collect();
    assert_eq!(
        columns,
        vec![("id", "INTEGER"), ("total", "NUMERIC(12, 2)"), ("two", "")]
    );
}

#[tokio::test]
async fn cancel_interrupts_a_running_statement_and_keeps_the_connection_usable() {
    let databases = TempDatabases::default();
    let connection = open_fixture(&databases).await;
    let cancel = connection.cancel_handle();
    let running = {
        let connection = Arc::clone(&connection);
        tokio::spawn(async move {
            connection
                .execute(INFINITE_QUERY, &ExecuteOptions::default())
                .await
        })
    };

    // Interrupting before the statement starts is a no-op, so keep trying until it stops.
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        while !running.is_finished() {
            cancel.cancel().await.unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        running.await.unwrap()
    })
    .await
    .unwrap();

    assert_eq!(result.err().map(|e| e.code), Some(DbErrorCode::Cancelled));
    assert_eq!(
        rows(&*connection, "SELECT 1").await,
        vec![vec![Value::Int(1)]]
    );
}

#[tokio::test]
async fn large_integers_and_null_survive_a_round_trip() {
    let databases = TempDatabases::default();
    let connection = open_fixture(&databases).await;

    let result = rows(&*connection, "SELECT 9007199254740993, NULL").await;

    assert_eq!(
        result,
        vec![vec![Value::Int(9_007_199_254_740_993), Value::Null]]
    );
}
