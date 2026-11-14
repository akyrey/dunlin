//! Spike 2.2 (MySQL/MariaDB): cost of abandoning a large result, with and without KILL QUERY.
//! `cargo test -p dbc-driver-mysql --test spike_early_stop -- --ignored --nocapture`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::time::{Duration, Instant};

use mysql_async::prelude::Queryable;
use mysql_async::{Conn, Opts, Row};
use testcontainers_modules::mariadb::Mariadb;
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, Image, ImageExt};

const SETUP: &str = "SET SESSION cte_max_recursion_depth = 10000000";
const ROWS: &str = "WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM s WHERE n < 3000000) \
                    SELECT n, md5(n) FROM s";
const LIMIT: usize = 1000;

async fn connect(port: u16) -> Conn {
    let url = format!("mysql://root@127.0.0.1:{port}/mysql");
    Conn::new(Opts::from_url(&url).unwrap()).await.unwrap()
}

/// Reads `LIMIT` rows and returns how long that took; the rest of the result is left unread.
async fn read_limit(conn: &mut Conn) -> Duration {
    let started = Instant::now();
    let mut result = conn.query_iter(ROWS).await.unwrap();
    let mut seen = 0;
    while seen < LIMIT {
        let Some(_row) = result.next().await.unwrap() else {
            break;
        };
        seen += 1;
    }
    drop(result);
    started.elapsed()
}

async fn next_query(conn: &mut Conn) -> Duration {
    let started = Instant::now();
    let _: Vec<Row> = conn.query("SELECT 1").await.unwrap_or_default();
    started.elapsed()
}

async fn measure<I: Image>(label: &str, container: &ContainerAsync<I>) {
    let port = container.get_host_port_ipv4(3306).await.unwrap();
    let mut conn = connect(port).await;
    let mut killer = connect(port).await;
    conn.query_drop(SETUP).await.ok();
    let id = conn.id();

    let first = read_limit(&mut conn).await;
    let after_drop = next_query(&mut conn).await;

    let second = read_limit(&mut conn).await;
    killer.query_drop(format!("KILL QUERY {id}")).await.unwrap();
    let after_kill = next_query(&mut conn).await;

    println!("{label}: first {LIMIT} rows {first:?}; next query after drop {after_drop:?}");
    println!("{label}: first {LIMIT} rows {second:?}; next query after KILL QUERY {after_kill:?}");
}

#[tokio::test]
#[ignore = "needs Docker; prints timings"]
async fn abandoning_versus_killing() {
    let mysql = Mysql::default().with_tag("8.4").start().await.unwrap();
    measure("mysql 8.4", &mysql).await;
    let mariadb = Mariadb::default().with_tag("11.4").start().await.unwrap();
    measure("mariadb 11.4", &mariadb).await;
}
