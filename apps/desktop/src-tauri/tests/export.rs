//! Export commands (tasks 8.2, 8.3, 8.5) over SQLite.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use dbc_core::WireValue;
use dbc_core::export::{CsvOptions, ExportFormat};
use dbc_core::result::{ColumnMeta, TypeCategory};
use dbc_core::schema::{NamespaceRef, TableRef};
use dbc_core::value::TextPayload;
use dunlin_lib::commands;
use dunlin_lib::ipc::{ExportResultArgs, ExportTableArgs, ExportTextArgs, SessionInfo, TabArgs};

use common::Fixture;

const BIG: usize = 250_000;

fn table(name: &str) -> TableRef {
    TableRef {
        namespace: NamespaceRef {
            catalog: None,
            schema: Some("main".into()),
        },
        name: name.into(),
    }
}

fn csv() -> ExportFormat {
    ExportFormat::Csv(CsvOptions::default())
}

fn table_args(
    session: &SessionInfo,
    name: &str,
    format: ExportFormat,
    path: &std::path::Path,
) -> ExportTableArgs {
    ExportTableArgs {
        session_id: session.session_id.clone(),
        view_id: "export-1".into(),
        table: table(name),
        filter: None,
        key: Vec::new(),
        sort: Vec::new(),
        format,
        path: path.to_string_lossy().into_owned(),
    }
}

fn big_database(fixture: &Fixture) -> std::path::PathBuf {
    let path = fixture.database();
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(&format!(
            "CREATE TABLE big (id INTEGER PRIMARY KEY, note TEXT);
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {BIG})
             INSERT INTO big SELECT i, 'row ' || i || ', with \"quotes\"' FROM n;"
        ))
        .unwrap();
    path
}

#[tokio::test]
async fn a_large_table_streams_to_csv_with_progress() {
    let fixture = Fixture::new();
    let session = fixture.session_on(&big_database(&fixture), false).await;
    let out = fixture.dir.path().join("big.csv");
    let progress = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&progress);

    let done = commands::export_table(
        &fixture.state,
        table_args(&session, "big", csv(), &out),
        move |rows| {
            seen.lock().unwrap().push(rows);
        },
    )
    .await
    .unwrap();

    let text = std::fs::read_to_string(&out).unwrap();
    let updates = progress.lock().unwrap().clone();
    assert_eq!(done.rows_written, BIG.to_string());
    assert_eq!(text.lines().count(), BIG + 1);
    assert!(text.starts_with("id,note\r\n1,\"row 1, with \"\"quotes\"\"\"\r\n"));
    assert!(updates.len() >= BIG / 1000 && updates.windows(2).all(|w| w[0] < w[1]));
    assert!(!fixture.dir.path().join("big.csv.partial").exists());
}

const NOTES: &str = "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT, amount NUMERIC, data BLOB, \
                     next INTEGER GENERATED ALWAYS AS (id + 1) STORED)";

type NoteRow = (i64, Option<String>, Option<String>, Option<Vec<u8>>, i64);

fn notes(c: &rusqlite::Connection) -> Vec<NoteRow> {
    let mut statement = c
        .prepare("SELECT id, body, CAST(amount AS TEXT), data, next FROM notes ORDER BY id")
        .unwrap();
    statement
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[tokio::test]
async fn exported_inserts_recreate_the_rows() {
    let fixture = Fixture::new();
    let source = fixture.database();
    rusqlite::Connection::open(&source)
        .unwrap()
        .execute_batch(&format!(
            "{NOTES}; INSERT INTO notes (id, body, amount, data) VALUES
             (1, 'it''s; DROP TABLE x; --', '12345678901234567890.123456789', X'DEAD00'),
             (2, NULL, NULL, NULL),
             (3, 'line one\nline two \\ back', '0.5', X'');"
        ))
        .unwrap();
    let session = fixture.session_on(&source, false).await;
    let out = fixture.dir.path().join("notes.sql");
    let format = ExportFormat::Insert {
        table: table("notes"),
        batch: 2,
    };

    commands::export_table(
        &fixture.state,
        table_args(&session, "notes", format, &out),
        |_| {},
    )
    .await
    .unwrap();
    let statements = std::fs::read_to_string(&out).unwrap();
    let target = rusqlite::Connection::open_in_memory().unwrap();
    target.execute_batch(NOTES).unwrap();
    target.execute_batch(&statements).unwrap();

    assert!(
        !statements.contains("\"next\""),
        "generated columns are not inserted"
    );
    assert_eq!(statements.matches("INSERT INTO").count(), 2);
    assert_eq!(
        notes(&target),
        notes(&rusqlite::Connection::open(&source).unwrap())
    );
}

#[tokio::test]
async fn a_cancelled_export_leaves_no_file() {
    let fixture = Fixture::new();
    let session = fixture.session_on(&big_database(&fixture), false).await;
    let out = fixture.dir.path().join("cancelled.csv");
    let args = ExportTableArgs {
        // Slow enough to cancel: every row computes a recursive count.
        filter: Some("(WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 20000) SELECT count(*) FROM c) > 0".into()),
        ..table_args(&session, "big", csv(), &out)
    };
    let export = commands::export_table(&fixture.state, args, |_| {});
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        commands::query_cancel(
            &fixture.state,
            TabArgs {
                session_id: session.session_id.clone(),
                tab_id: "export-1".into(),
            },
        )
        .await
        .unwrap();
    };

    let (result, ()) = tokio::time::timeout(Duration::from_secs(20), async {
        tokio::join!(export, cancel)
    })
    .await
    .unwrap();

    assert_eq!(result.unwrap_err().code, "cancelled");
    assert!(!out.exists());
    assert!(!fixture.dir.path().join("cancelled.csv.partial").exists());
}

fn text(v: &str, truncated: bool) -> WireValue {
    WireValue::Text(TextPayload {
        v: v.into(),
        truncated,
    })
}

fn columns() -> Vec<ColumnMeta> {
    vec![ColumnMeta {
        name: "note".into(),
        type_name: "TEXT".into(),
        category: TypeCategory::Text,
        nullable: None,
    }]
}

#[tokio::test]
async fn results_export_to_files_and_text_unless_cut_short() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let out = fixture.dir.path().join("result.json");
    let args = |rows: Vec<Vec<WireValue>>, path: &str| ExportResultArgs {
        session_id: session.session_id.clone(),
        columns: columns(),
        rows,
        format: ExportFormat::Json,
        path: path.into(),
    };

    let done = commands::export_result(
        &fixture.state,
        args(vec![vec![text("a", false)]], out.to_str().unwrap()),
    )
    .await
    .unwrap();
    let cut = commands::export_result(
        &fixture.state,
        args(vec![vec![text("a", true)]], out.to_str().unwrap()),
    )
    .await
    .unwrap_err();
    let relative = commands::export_result(&fixture.state, args(Vec::new(), "out.json"))
        .await
        .unwrap_err();
    let missing_folder =
        commands::export_result(&fixture.state, args(Vec::new(), "/no/such/folder/out.json"))
            .await
            .unwrap_err();
    let copied = commands::export_text(
        &fixture.state,
        ExportTextArgs {
            session_id: session.session_id.clone(),
            columns: columns(),
            rows: vec![vec![text("it's", false)]],
            format: ExportFormat::Insert {
                table: table("notes"),
                batch: 10,
            },
        },
    )
    .await
    .unwrap();

    assert_eq!(done.rows_written, "1");
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "[\n  {\"note\": \"a\"}\n]\n"
    );
    assert_eq!(cut.code, "invalid_argument");
    assert_eq!(relative.code, "invalid_argument");
    assert_eq!(missing_folder.code, "invalid_argument");
    assert_eq!(
        copied,
        "INSERT INTO \"main\".\"notes\" (\"note\") VALUES\n('it''s');\n"
    );
}
