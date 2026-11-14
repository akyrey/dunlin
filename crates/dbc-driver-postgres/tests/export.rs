//! Phase 8 acceptance on PostgreSQL: exported CSV re-imports with `COPY`, exported `INSERT`
//! statements reproduce the rows, and a 5 million row export streams and can be stopped.
//! Needs Docker.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dbc_core::dialect::Dialect;
use dbc_core::driver::{Connection, Driver};
use dbc_core::export::{CsvOptions, ExportFormat, ExportWriter};
use dbc_core::result::StreamEvent;
use dbc_core::schema::{NamespaceRef, TableRef};
use dbc_driver_postgres::PostgresDriver;
use futures_util::SinkExt;
use tokio::sync::mpsc;

use common::Server;

const TYPES: &str = "id integer PRIMARY KEY, note text, amount numeric, big bigint, ratio double precision, \
                     flag boolean, data bytea, doc jsonb, at timestamp, day date, tags text[], u uuid";

const ROWS: &str = "
    INSERT INTO source VALUES
      (1, 'it''s, \"quoted\"; DROP TABLE x; --', 12345678901234567890.123456789, 9223372036854775807,
       1.5, true, '\\xdead00', '{\"a\": [1, \"b\"]}', '2026-11-14 10:00:00.123456', '2026-11-14',
       '{a,\"b c\",NULL}', '6f1c2d3e-0000-4000-8000-000000000001'),
      (2, E'line one\\nline two \\\\ back', -0.5, -1, -2.25, false, '\\x', 'null', NULL, NULL, '{}', NULL),
      (3, '', NULL, NULL, 'NaN', NULL, NULL, NULL, NULL, NULL, NULL, NULL),
      (4, NULL, 0, 0, 0, NULL, NULL, '[]', '0001-01-01 00:00:00', '9999-12-31', NULL, NULL);";

fn table(name: &str) -> TableRef {
    TableRef {
        namespace: NamespaceRef {
            catalog: None,
            schema: Some("public".into()),
        },
        name: name.into(),
    }
}

/// Exports `sql` to bytes in `format`, through `stream_rows`.
async fn export(
    connection: &dyn Connection,
    dialect: Arc<dyn Dialect>,
    sql: &str,
    format: ExportFormat,
) -> Vec<u8> {
    let (tx, mut rx) = mpsc::channel(4);
    let writer = async {
        let mut writer = ExportWriter::new(Vec::new(), format);
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Columns(columns) => writer.begin(&columns).unwrap(),
                StreamEvent::Rows(rows) => writer.rows(&rows, dialect.as_ref()).unwrap(),
            }
        }
        writer.finish().unwrap()
    };
    let (sent, bytes) = tokio::join!(connection.stream_rows(sql, &[], tx), writer);
    sent.unwrap();
    bytes
}

async fn same_rows(admin: &tokio_postgres::Client, a: &str, b: &str) {
    let difference: i64 = admin
        .query_one(
            &format!("SELECT count(*) FROM ((TABLE {a} EXCEPT ALL TABLE {b}) UNION ALL (TABLE {b} EXCEPT ALL TABLE {a})) d"),
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let rows: i64 = admin
        .query_one(&format!("SELECT count(*) FROM {b}"), &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!((difference, rows), (0, 4), "{a} and {b} differ");
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn csv_reimports_with_copy_and_inserts_reproduce_the_rows() {
    let server = Server::start().await;
    let database = server.fresh_database().await;
    let admin = server.admin(&database).await;
    admin
        .batch_execute(&format!(
            "CREATE TABLE source ({TYPES}); CREATE TABLE from_csv ({TYPES}); CREATE TABLE from_insert ({TYPES}); {ROWS}"
        ))
        .await
        .unwrap();
    let connection = PostgresDriver
        .connect(&server.config(&database, false))
        .await
        .unwrap();
    let dialect = PostgresDriver.dialect();
    let select = "SELECT * FROM source ORDER BY id";

    let csv = export(
        connection.as_ref(),
        Arc::clone(&dialect),
        select,
        ExportFormat::Csv(CsvOptions::default()),
    )
    .await;
    let sink = admin
        .copy_in("COPY from_csv FROM STDIN (FORMAT csv, HEADER true)")
        .await
        .unwrap();
    futures_util::pin_mut!(sink);
    sink.send(bytes::Bytes::from(csv)).await.unwrap();
    sink.finish().await.unwrap();
    let inserts = export(
        connection.as_ref(),
        dialect,
        select,
        ExportFormat::Insert {
            table: table("from_insert"),
            batch: 3,
        },
    )
    .await;
    admin
        .batch_execute(std::str::from_utf8(&inserts).unwrap())
        .await
        .unwrap();

    same_rows(&admin, "source", "from_csv").await;
    same_rows(&admin, "source", "from_insert").await;
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn five_million_rows_stream_to_a_file_and_stop_early() {
    let server = Server::start().await;
    let database = server.fresh_database().await;
    let connection = PostgresDriver
        .connect(&server.config(&database, false))
        .await
        .unwrap();
    let dialect = PostgresDriver.dialect();
    let sql = "SELECT n, 'row ' || n AS note FROM generate_series(1, 5000000) n";
    let mut file = tempfile::tempfile().unwrap();

    let (tx, mut rx) = mpsc::channel(4);
    let writer = async {
        let mut writer = ExportWriter::new(
            std::io::BufWriter::new(&mut file),
            ExportFormat::Csv(CsvOptions::default()),
        );
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Columns(columns) => writer.begin(&columns).unwrap(),
                StreamEvent::Rows(rows) => writer.rows(&rows, dialect.as_ref()).unwrap(),
            }
        }
        writer.rows_written()
    };
    let (sent, written) = tokio::join!(connection.stream_rows(sql, &[], tx), writer);
    file.flush().unwrap();
    assert_eq!(sent.unwrap(), 5_000_000);
    assert_eq!(written, 5_000_000);

    // A reader that stops after 10 000 rows: the statement is cancelled, not drained.
    let (tx, mut rx) = mpsc::channel(1);
    let started = Instant::now();
    let reader = async {
        let mut rows = 0;
        while let Some(event) = rx.recv().await {
            if let StreamEvent::Rows(batch) = event {
                rows += batch.len();
                if rows >= 10_000 {
                    break;
                }
            }
        }
        drop(rx);
    };
    let (stopped, ()) = tokio::join!(connection.stream_rows(sql, &[], tx), reader);
    assert!(stopped.unwrap() < 5_000_000);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "stopping took {:?}",
        started.elapsed()
    );
    assert!(
        connection
            .execute("SELECT 1", &Default::default())
            .await
            .is_ok()
    );
}
