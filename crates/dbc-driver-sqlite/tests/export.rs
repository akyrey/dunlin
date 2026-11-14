//! Phase 8 acceptance on SQLite: exported CSV re-imports with the `sqlite3` shell's `.import`.
//! `.import` has no way to write `NULL` (empty fields become empty strings), so these rows
//! have none. Skipped when `sqlite3` is not installed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::process::Command;

use dbc_core::driver::Driver;
use dbc_core::export::{CsvOptions, ExportFormat, ExportWriter};
use dbc_core::result::StreamEvent;
use dbc_driver_sqlite::SqliteDriver;
use tokio::sync::mpsc;

use common::{TempDatabases, config};

const TABLE: &str =
    "CREATE TABLE source (id INTEGER PRIMARY KEY, note TEXT, amount REAL, big INTEGER)";

#[tokio::test]
async fn csv_reimports_with_the_sqlite3_shell() {
    if Command::new("sqlite3").arg("--version").output().is_err() {
        return;
    }
    let databases = TempDatabases::default();
    let dir = databases.dir();
    let source = dir.join("source.db");
    rusqlite::Connection::open(&source)
        .unwrap()
        .execute_batch(&format!(
            "{TABLE}; INSERT INTO source VALUES
             (1, 'it''s, \"quoted\"; DROP TABLE x; --', 1.5, 9223372036854775807),
             (2, 'line one\nline two \\ back', -0.25, -1),
             (3, '', 0, 0),
             (4, 'naïve 😀', 1e100, 7);"
        ))
        .unwrap();
    let connection = SqliteDriver.connect(&config(&source, true)).await.unwrap();
    let dialect = SqliteDriver.dialect();
    let (tx, mut rx) = mpsc::channel(4);
    let writer = async {
        let mut writer = ExportWriter::new(Vec::new(), ExportFormat::Csv(CsvOptions::default()));
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Columns(columns) => writer.begin(&columns).unwrap(),
                StreamEvent::Rows(rows) => writer.rows(&rows, dialect.as_ref()).unwrap(),
            }
        }
        writer.finish().unwrap()
    };
    let (sent, csv) = tokio::join!(
        connection.stream_rows("SELECT * FROM source ORDER BY id", &[], tx),
        writer
    );
    assert_eq!(sent.unwrap(), 4);
    let file = dir.join("source.csv");
    std::fs::write(&file, csv).unwrap();

    let target = dir.join("target.db");
    rusqlite::Connection::open(&target)
        .unwrap()
        .execute_batch(TABLE)
        .unwrap();
    let import = Command::new("sqlite3")
        .arg(&target)
        .arg(format!(".import --csv --skip 1 {} source", file.display()))
        .output()
        .unwrap();
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );

    let rows = |path: &std::path::Path| -> Vec<(i64, String, f64, i64)> {
        let c = rusqlite::Connection::open(path).unwrap();
        let mut statement = c
            .prepare("SELECT id, note, amount, big FROM source ORDER BY id")
            .unwrap();
        statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(rows(&target), rows(&source));
}
