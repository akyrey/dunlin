//! A 10 million row table opens with one indexed page query and an immediate estimate
//! (Phase 6 acceptance). Needs Docker; takes about half a minute to load.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::time::{Duration, Instant};

use dbc_core::data::{self, DEFAULT_PAGE_SIZE, RowCount, SortKey, TablePageRequest};
use dbc_core::driver::Driver;
use dbc_core::schema::{NamespaceRef, TableRef};
use dbc_driver_postgres::PostgresDriver;

use common::Server;

const ROWS: u64 = 10_000_000;

#[tokio::test]
#[ignore = "needs Docker"]
async fn ten_million_rows_open_in_one_page_query() {
    let server = Server::start().await;
    let database = server.fresh_database().await;
    server
        .admin(&database)
        .await
        .batch_execute(&format!(
            "CREATE TABLE big (id bigint PRIMARY KEY, note text);
             INSERT INTO big SELECT n, 'row ' || n FROM generate_series(1, {ROWS}) n;
             ANALYZE big;"
        ))
        .await
        .unwrap();
    let connection = PostgresDriver
        .connect(&server.config(&database, false))
        .await
        .unwrap();
    let dialect = PostgresDriver.dialect();
    let table = TableRef {
        namespace: NamespaceRef {
            catalog: None,
            schema: Some("public".into()),
        },
        name: "big".into(),
    };
    let info = connection.describe_table(&table).await.unwrap();
    let request = TablePageRequest {
        table: table.clone(),
        filter: None,
        key: Vec::new(),
        sort: Vec::new(),
        limit: DEFAULT_PAGE_SIZE,
        offset: 0,
    };

    let started = Instant::now();
    let first = data::fetch_page(connection.as_ref(), dialect.as_ref(), &info, &request)
        .await
        .unwrap();
    let estimate = data::count_rows(
        connection.as_ref(),
        dialect.as_ref(),
        &info,
        None,
        &[],
        false,
    )
    .await
    .unwrap();
    let opened = started.elapsed();
    let last = data::fetch_page(
        connection.as_ref(),
        dialect.as_ref(),
        &info,
        &TablePageRequest {
            sort: vec![SortKey {
                column: "id".into(),
                descending: true,
            }],
            ..request
        },
    )
    .await
    .unwrap();

    assert_eq!(first.rows.len(), DEFAULT_PAGE_SIZE);
    assert_eq!(last.rows[0][0], dbc_core::Value::Int(10_000_000));
    let RowCount::Estimate(rows) = estimate else {
        panic!("expected an estimate, got {estimate:?}");
    };
    assert!(rows.abs_diff(ROWS) < ROWS / 10, "estimate {rows}");
    assert!(
        opened < Duration::from_millis(500),
        "opening took {opened:?}"
    );
}
