//! `import_preview` and `import_csv` over a SQLite fixture (D-034).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::sync::{Arc, Mutex};

use dbc_core::WireValue;
use dbc_core::export::CsvOptions;
use dbc_core::schema::{NamespaceRef, TableRef};
use dbc_core::value::TextPayload;
use dunlin_lib::commands;
use dunlin_lib::ipc::{
    ImportColumn, ImportCsvArgs, ImportPreviewArgs, KeyArg, SessionInfo, TablePageArgs,
};

use common::Fixture;

fn customers() -> TableRef {
    TableRef {
        namespace: NamespaceRef {
            catalog: None,
            schema: Some("main".into()),
        },
        name: "customers".into(),
    }
}

fn csv_file(fixture: &Fixture, text: &str) -> String {
    let path = fixture
        .dir
        .path()
        .join(format!("{}.csv", uuid::Uuid::new_v4()));
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}

fn args(session: &SessionInfo, path: String) -> ImportCsvArgs {
    ImportCsvArgs {
        session_id: session.session_id.clone(),
        import_id: "import-1".into(),
        table: customers(),
        path,
        options: CsvOptions::default(),
        columns: vec![
            ImportColumn {
                column: "id".into(),
                source: 1,
            },
            ImportColumn {
                column: "name".into(),
                source: 0,
            },
        ],
    }
}

async fn name_of(fixture: &Fixture, session: &SessionInfo, id: i64) -> Option<WireValue> {
    let page = commands::table_page(
        &fixture.state,
        TablePageArgs {
            session_id: session.session_id.clone(),
            view_id: "view-1".into(),
            table: customers(),
            filter: None,
            key: vec![KeyArg {
                column: "id".into(),
                value: WireValue::Int { v: id.to_string() },
            }],
            sort: Vec::new(),
            limit: 1,
            offset: 0,
            after: None,
        },
    )
    .await
    .unwrap();
    page.rows.first().map(|row| row[1].clone())
}

#[tokio::test]
async fn previews_the_first_records_with_their_header() {
    let fixture = Fixture::new();
    let path = csv_file(&fixture, "name;id\nAda;1\n;2\n");

    let preview = commands::import_preview(ImportPreviewArgs {
        path,
        options: CsvOptions {
            delimiter: ';',
            ..CsvOptions::default()
        },
    })
    .await
    .unwrap();
    let missing = commands::import_preview(ImportPreviewArgs {
        path: fixture
            .dir
            .path()
            .join("nope.csv")
            .to_string_lossy()
            .into_owned(),
        options: CsvOptions::default(),
    })
    .await
    .unwrap_err();

    assert_eq!(preview.headers, vec!["name", "id"]);
    assert_eq!(preview.rows[1], vec![None, Some("2".to_owned())]);
    assert_eq!(missing.code, "import");
}

#[tokio::test]
async fn imports_every_record_and_reports_progress() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let rows: String = (1000..3500).map(|id| format!("Name {id},{id}\n")).collect();
    let path = csv_file(&fixture, &format!("name,id\n{rows}"));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let heard = Arc::clone(&seen);

    let done = commands::import_csv(&fixture.state, args(&session, path), move |n| {
        heard.lock().unwrap().push(n);
    })
    .await
    .unwrap();

    assert_eq!(done.rows_inserted, "2500");
    assert_eq!(*seen.lock().unwrap(), vec![1000, 2000, 2500]);
    assert_eq!(
        name_of(&fixture, &session, 3499).await,
        Some(WireValue::Text(TextPayload {
            v: "Name 3499".into(),
            truncated: false
        }))
    );
}

#[tokio::test]
async fn a_refused_row_rolls_the_import_back_and_names_its_lines() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    // Customer 1 exists in the fixture.
    let path = csv_file(&fixture, "name,id\nNew,500\nDuplicate,1\n");
    let bad_value = csv_file(&fixture, "name,id\nNew,500\nOops,x\n");

    let refused = commands::import_csv(&fixture.state, args(&session, path), |_| {})
        .await
        .unwrap_err();
    let invalid = commands::import_csv(&fixture.state, args(&session, bad_value), |_| {})
        .await
        .unwrap_err();

    assert_eq!(refused.code, "constraint");
    assert!(
        refused.message.starts_with("Lines 2–3: "),
        "{}",
        refused.message
    );
    assert_eq!(name_of(&fixture, &session, 500).await, None);
    assert_eq!(invalid.code, "invalid_argument");
    assert_eq!(
        invalid.message,
        "Line 3, column id: \"x\" is not a whole number"
    );
}

#[tokio::test]
async fn read_only_sessions_and_bad_mappings_are_refused() {
    let fixture = Fixture::new();
    let read_only = fixture.session(true).await;
    let session = fixture.session(false).await;
    let path = csv_file(&fixture, "name,id\nA,9\n");

    let refused = commands::import_csv(&fixture.state, args(&read_only, path.clone()), |_| {})
        .await
        .unwrap_err();
    let twice = ImportCsvArgs {
        columns: vec![
            ImportColumn {
                column: "name".into(),
                source: 0,
            },
            ImportColumn {
                column: "name".into(),
                source: 1,
            },
        ],
        ..args(&session, path.clone())
    };
    let unknown = ImportCsvArgs {
        columns: vec![ImportColumn {
            column: "nope".into(),
            source: 0,
        }],
        ..args(&session, path)
    };

    assert_eq!(refused.code, "read_only");
    let twice = commands::import_csv(&fixture.state, twice, |_| {})
        .await
        .unwrap_err();
    assert!(twice.message.contains("more than once"));
    let unknown = commands::import_csv(&fixture.state, unknown, |_| {})
        .await
        .unwrap_err();
    assert_eq!(unknown.code, "not_found");
}
