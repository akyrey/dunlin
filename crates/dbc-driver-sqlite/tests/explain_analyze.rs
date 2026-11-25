//! `explain_analyze` on SQLite (task 10.5): the plan plus a measured total; writes are undone.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use dbc_core::Value;
use dbc_core::driver::Driver;
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_driver_sqlite::SqliteDriver;

use common::{TempDatabases, config};

async fn count(connection: &dyn dbc_core::driver::Connection) -> Value {
    match connection
        .execute("SELECT count(*) FROM event_log", &ExecuteOptions::default())
        .await
        .unwrap()
    {
        StatementOutcome::Rows(result) => result.rows[0][0].clone(),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_read_gets_its_plan_and_a_total_time() {
    let databases = TempDatabases::default();
    let connection = SqliteDriver
        .connect(&config(databases.fixture(), false))
        .await
        .unwrap();
    let analyzed = connection
        .explain_analyze("SELECT * FROM customers WHERE name = 'Ada';", false)
        .await
        .unwrap();
    assert!(analyzed.total_ms.unwrap() >= 0.0);
    let scan = if analyzed.plan.children.is_empty() {
        &analyzed.plan
    } else {
        &analyzed.plan.children[0]
    };
    assert!(scan.full_scan, "{:?}", analyzed.plan);
    assert_eq!(scan.table.as_deref(), Some("customers"));
}

#[tokio::test]
async fn a_write_is_rolled_back() {
    let databases = TempDatabases::default();
    let connection = SqliteDriver
        .connect(&config(databases.fixture(), false))
        .await
        .unwrap();
    let before = count(connection.as_ref()).await;
    connection
        .explain_analyze("DELETE FROM event_log", true)
        .await
        .unwrap();
    assert_eq!(count(connection.as_ref()).await, before);
}

#[tokio::test]
async fn a_failing_statement_still_rolls_back() {
    let databases = TempDatabases::default();
    let connection = SqliteDriver
        .connect(&config(databases.fixture(), false))
        .await
        .unwrap();
    let error = connection
        .explain_analyze("INSERT INTO customers (id, name) VALUES (1, 'dup')", true)
        .await;
    assert!(error.is_err());
    // The connection is usable and outside any transaction.
    connection
        .execute("BEGIN", &ExecuteOptions::default())
        .await
        .unwrap();
    connection
        .execute("ROLLBACK", &ExecuteOptions::default())
        .await
        .unwrap();
}
