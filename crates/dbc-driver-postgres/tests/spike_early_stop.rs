//! Spike 2.2: cost of abandoning a large result at the row limit, with and without cancelling.
//! `cargo test -p dbc-driver-postgres --test spike_early_stop -- --ignored --nocapture`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::time::{Duration, Instant};

use futures_util::{StreamExt, pin_mut};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_postgres::{Client, NoTls};

const ROWS: &str = "SELECT g, md5(g::text) FROM generate_series(1, 5000000) g";
const LIMIT: usize = 1000;

async fn read_limit(client: &Client) -> Duration {
    let started = Instant::now();
    let statement = client.prepare(ROWS).await.unwrap();
    let stream = client
        .query_raw(&statement, Vec::<i32>::new())
        .await
        .unwrap();
    pin_mut!(stream);
    for _ in 0..LIMIT {
        stream.next().await.unwrap().unwrap();
    }
    started.elapsed()
}

async fn next_query(client: &Client) -> Duration {
    let started = Instant::now();
    client.simple_query("SELECT 1").await.unwrap();
    started.elapsed()
}

#[tokio::test]
#[ignore = "needs Docker; prints timings"]
async fn abandoning_versus_cancelling() {
    let container = Postgres::default()
        .with_tag("18-alpine")
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("host=127.0.0.1 port={port} user=postgres password=postgres dbname=postgres");
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    tokio::spawn(connection);

    let first = read_limit(&client).await;
    let after_drop = next_query(&client).await;

    let second = read_limit(&client).await;
    client.cancel_token().cancel_query(NoTls).await.unwrap();
    let after_cancel = next_query(&client).await;

    println!("postgres: first {LIMIT} rows {first:?}; next query after drop {after_drop:?}");
    println!("postgres: first {LIMIT} rows {second:?}; next query after cancel {after_cancel:?}");
}
