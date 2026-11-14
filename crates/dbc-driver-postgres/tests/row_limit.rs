//! Stopping at the row limit (D-017) on results that end at awkward moments. Needs Docker.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::time::{Duration, Instant};

use dbc_core::driver::Driver;
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_driver_postgres::PostgresDriver;

use common::Server;

const LIMIT: usize = 5;

#[tokio::test]
#[ignore = "needs Docker"]
async fn truncated_results_never_cancel_the_next_statement() {
    let server = Server::start().await;
    let connection = PostgresDriver
        .connect(&server.config("postgres", false))
        .await
        .unwrap();
    let options = ExecuteOptions { row_limit: LIMIT };
    // Sizes around the drain-before-cancel threshold, where the statement ends while the cancel
    // is in flight, plus one far beyond it.
    let sizes = [LIMIT + 1, 10_000, 10_006, 10_010, 12_000, 50_000_000];

    for round in 0..10 {
        for size in sizes {
            let started = Instant::now();
            let sql = format!("SELECT generate_series(1, {size})");
            let outcome = connection.execute(&sql, &options).await;
            let after = connection
                .execute("SELECT 1", &ExecuteOptions::default())
                .await;

            let Ok(StatementOutcome::Rows(result)) = outcome else {
                panic!("round {round}, {size} rows: {outcome:?}");
            };
            assert!(result.truncated && result.rows.len() == LIMIT);
            assert!(
                after.is_ok(),
                "round {round}, {size} rows: next statement {after:?}"
            );
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "{size} rows took {:?}",
                started.elapsed()
            );
        }
    }
}
