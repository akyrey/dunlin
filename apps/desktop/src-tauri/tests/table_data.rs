//! `table_page`, `table_count` and `cell_full_value` over a SQLite fixture (task 6.4).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::time::Duration;

use dbc_core::WireValue;
use dbc_core::data::{RowCount, SortKey};
use dbc_core::schema::{NamespaceRef, TableRef};
use dunlin_lib::commands;
use dunlin_lib::ipc::{
    CellFullValueArgs, KeyArg, SessionInfo, TabArgs, TableCountArgs, TablePageArgs,
};

use common::Fixture;

fn table(name: &str) -> TableRef {
    TableRef {
        namespace: NamespaceRef {
            catalog: None,
            schema: Some("main".into()),
        },
        name: name.into(),
    }
}

fn page(session: &SessionInfo, name: &str) -> TablePageArgs {
    TablePageArgs {
        session_id: session.session_id.clone(),
        view_id: "view-1".into(),
        table: table(name),
        filter: None,
        key: Vec::new(),
        sort: Vec::new(),
        limit: 2,
        offset: 0,
    }
}

fn int(v: i64) -> WireValue {
    WireValue::Int { v: v.to_string() }
}

fn first_column(page: &dunlin_lib::ipc::TablePage) -> Vec<WireValue> {
    page.rows.iter().map(|row| row[0].clone()).collect()
}

#[tokio::test]
async fn pages_through_a_table_and_says_when_more_follow() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;

    let first = commands::table_page(&fixture.state, page(&session, "orders"))
        .await
        .unwrap();
    let second = commands::table_page(
        &fixture.state,
        TablePageArgs {
            offset: 2,
            ..page(&session, "orders")
        },
    )
    .await
    .unwrap();

    assert_eq!(first_column(&first), vec![int(10), int(11)]);
    assert!(first.has_more);
    assert_eq!(first_column(&second), vec![int(12)]);
    assert!(!second.has_more);
    assert_eq!(first.columns[0].name, "id");
}

#[tokio::test]
async fn sorts_filters_and_follows_a_key() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;

    let sorted = commands::table_page(
        &fixture.state,
        TablePageArgs {
            sort: vec![SortKey {
                column: "total".into(),
                descending: true,
            }],
            limit: 10,
            ..page(&session, "orders")
        },
    )
    .await
    .unwrap();
    let keyed = commands::table_page(
        &fixture.state,
        TablePageArgs {
            key: vec![KeyArg {
                column: "id".into(),
                value: int(2),
            }],
            ..page(&session, "customers")
        },
    )
    .await
    .unwrap();
    let bad = commands::table_page(
        &fixture.state,
        TablePageArgs {
            filter: Some("total >>> 1".into()),
            ..page(&session, "orders")
        },
    )
    .await
    .unwrap_err();
    let unknown_column = commands::table_page(
        &fixture.state,
        TablePageArgs {
            sort: vec![SortKey {
                column: "nope".into(),
                descending: false,
            }],
            ..page(&session, "orders")
        },
    )
    .await
    .unwrap_err();

    assert_eq!(first_column(&sorted), vec![int(12), int(10), int(11)]);
    assert_eq!(first_column(&keyed), vec![int(2)]);
    assert_eq!(bad.code, "syntax");
    assert_eq!(unknown_column.code, "not_found");
}

#[tokio::test]
async fn counts_exactly_or_reports_no_estimate() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let count = |filter: Option<&str>, exact: bool| TableCountArgs {
        session_id: session.session_id.clone(),
        view_id: "view-1".into(),
        table: table("orders"),
        filter: filter.map(str::to_owned),
        key: Vec::new(),
        exact,
    };

    let estimate = commands::table_count(&fixture.state, count(None, false))
        .await
        .unwrap();
    let exact = commands::table_count(&fixture.state, count(Some("total > 1"), true))
        .await
        .unwrap();

    assert_eq!(estimate, RowCount::Unknown);
    assert_eq!(exact, RowCount::Exact(2));
}

#[tokio::test]
async fn a_truncated_cell_is_fetched_whole_by_its_row_key() {
    let fixture = Fixture::new();
    let path = fixture.database();
    let big = "x".repeat(commands::PAGE_DISPLAY_LIMIT + 10);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute("INSERT INTO tags (code, label) VALUES ('big', ?1)", [&big])
        .unwrap();
    let session = fixture.session_on(&path, false).await;
    let shown = commands::table_page(
        &fixture.state,
        TablePageArgs {
            key: vec![KeyArg {
                column: "code".into(),
                value: WireValue::Text(dbc_core::value::TextPayload {
                    v: "big".into(),
                    truncated: false,
                }),
            }],
            ..page(&session, "tags")
        },
    )
    .await
    .unwrap();
    let cell = |key: Vec<KeyArg>| CellFullValueArgs {
        session_id: session.session_id.clone(),
        view_id: "view-1".into(),
        table: table("tags"),
        key,
        column: "label".into(),
    };

    let full = commands::cell_full_value(
        &fixture.state,
        cell(vec![KeyArg {
            column: "code".into(),
            value: shown.rows[0][0].clone(),
        }]),
    )
    .await
    .unwrap();
    let without_key = commands::cell_full_value(&fixture.state, cell(Vec::new()))
        .await
        .unwrap_err();

    assert!(matches!(&shown.rows[0][1], WireValue::Text(p) if p.truncated));
    assert!(matches!(full, WireValue::Text(p) if !p.truncated && p.v == big));
    assert_eq!(without_key.code, "unsupported");
}

#[tokio::test]
async fn a_slow_page_can_be_cancelled_and_does_not_block_the_tree() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let slow = commands::table_page(
        &fixture.state,
        TablePageArgs {
            filter: Some(
                "(WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c) > 0"
                    .into(),
            ),
            ..page(&session, "orders")
        },
    );
    let other = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let tree = commands::schema_objects(
            &fixture.state,
            dunlin_lib::ipc::SchemaObjectsArgs {
                session_id: session.session_id.clone(),
                namespace: table("x").namespace,
            },
        )
        .await;
        let cancel = commands::query_cancel(
            &fixture.state,
            TabArgs {
                session_id: session.session_id.clone(),
                tab_id: "view-1".into(),
            },
        )
        .await;
        (tree, cancel)
    };

    let (slow, (tree, cancel)) =
        tokio::time::timeout(Duration::from_secs(5), async { tokio::join!(slow, other) })
            .await
            .unwrap();

    assert!(tree.is_ok() && cancel.is_ok());
    assert_eq!(slow.unwrap_err().code, "cancelled");
}

#[tokio::test]
async fn page_sizes_are_bounded() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;

    let error = commands::table_page(
        &fixture.state,
        TablePageArgs {
            limit: 0,
            ..page(&session, "orders")
        },
    )
    .await
    .unwrap_err();

    assert_eq!(error.code, "invalid_argument");
}
