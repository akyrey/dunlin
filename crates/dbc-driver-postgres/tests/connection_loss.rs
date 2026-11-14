//! The server ends our session: the next call reports `ConnectionLost`, and a new connection works.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use dbc_core::DbErrorCode;
use dbc_core::Value;
use dbc_core::driver::Driver;
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_driver_postgres::PostgresDriver;

use common::Server;

#[tokio::test]
#[ignore = "needs Docker"]
async fn a_terminated_backend_is_reported_as_connection_lost() {
    let server = Server::start().await;
    let config = server.config("postgres", false);
    let connection = PostgresDriver.connect(&config).await.unwrap();
    let options = ExecuteOptions::default();
    let StatementOutcome::Rows(pid) = connection
        .execute("SELECT pg_backend_pid()", &options)
        .await
        .unwrap()
    else {
        panic!("expected rows")
    };
    let Value::Int(pid) = pid.rows[0][0] else {
        panic!("expected an integer pid")
    };
    server
        .admin("postgres")
        .await
        .execute("SELECT pg_terminate_backend($1)", &[&(pid as i32)])
        .await
        .unwrap();

    let lost = connection
        .execute("SELECT 1", &options)
        .await
        .err()
        .map(|e| e.code);
    let again = connection
        .execute("SELECT 1", &options)
        .await
        .err()
        .map(|e| e.code);
    let fresh = PostgresDriver
        .connect(&config)
        .await
        .unwrap()
        .execute("SELECT 1", &options)
        .await;

    assert_eq!(lost, Some(DbErrorCode::ConnectionLost));
    assert_eq!(again, Some(DbErrorCode::ConnectionLost));
    assert!(fresh.is_ok());
}
