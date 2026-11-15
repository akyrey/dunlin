//! `changes_preview` and `changes_apply` (task 7.5), over a SQLite fixture.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use dbc_core::WireValue;
use dbc_core::schema::{NamespaceRef, TableRef};
use dbc_core::value::TextPayload;
use dunlin_lib::commands;
use dunlin_lib::ipc::{ChangesArgs, KeyArg, OperationArg, SessionInfo, TablePageArgs};

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

fn int(v: i64) -> WireValue {
    WireValue::Int { v: v.to_string() }
}

fn text(v: &str) -> WireValue {
    WireValue::Text(TextPayload {
        v: v.into(),
        truncated: false,
    })
}

fn kv(column: &str, value: WireValue) -> KeyArg {
    KeyArg {
        column: column.into(),
        value,
    }
}

fn args(session: &SessionInfo, name: &str, operations: Vec<OperationArg>) -> ChangesArgs {
    ChangesArgs {
        session_id: session.session_id.clone(),
        view_id: "view-1".into(),
        table: table(name),
        operations,
    }
}

async fn names(fixture: &Fixture, session: &SessionInfo) -> Vec<WireValue> {
    let page = commands::table_page(
        &fixture.state,
        TablePageArgs {
            session_id: session.session_id.clone(),
            view_id: "view-2".into(),
            table: table("customers"),
            filter: None,
            key: Vec::new(),
            sort: Vec::new(),
            limit: 100,
            offset: 0,
            after: None,
        },
    )
    .await
    .unwrap();
    page.rows.into_iter().map(|row| row[1].clone()).collect()
}

#[tokio::test]
async fn previews_then_applies_a_batch() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let operations = vec![
        OperationArg::Update {
            key: vec![kv("id", int(1))],
            set: vec![kv("name", text("Ada'; DROP TABLE customers; --"))],
        },
        OperationArg::Insert {
            values: vec![kv("id", int(4)), kv("name", text("Linus"))],
        },
        OperationArg::Delete {
            key: vec![kv("id", int(3))],
        },
    ];

    let preview = commands::changes_preview(
        &fixture.state,
        args(&session, "customers", operations.clone()),
    )
    .await
    .unwrap();
    let before = names(&fixture, &session).await;
    let applied = commands::changes_apply(&fixture.state, args(&session, "customers", operations))
        .await
        .unwrap();
    let after = names(&fixture, &session).await;

    assert_eq!(
        preview,
        vec![
            r#"DELETE FROM "main"."customers" WHERE "id" = 3"#,
            r#"UPDATE "main"."customers" SET "name" = 'Ada''; DROP TABLE customers; --' WHERE "id" = 1"#,
            r#"INSERT INTO "main"."customers" ("id", "name") VALUES (4, 'Linus')"#,
        ]
    );
    assert_eq!(before.len(), 3, "nothing was written before Apply");
    assert_eq!(applied.statements, 3);
    assert_eq!(applied.rows_affected, "3");
    assert_eq!(
        after,
        vec![
            text("Ada'; DROP TABLE customers; --"),
            text("Grace"),
            text("Linus")
        ]
    );
}

#[tokio::test]
async fn a_failed_apply_names_the_operation_and_changes_nothing() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let operations = vec![
        OperationArg::Update {
            key: vec![kv("id", int(1))],
            set: vec![kv("name", text("changed"))],
        },
        OperationArg::Delete {
            key: vec![kv("id", int(99))],
        },
    ];

    let error = commands::changes_apply(&fixture.state, args(&session, "customers", operations))
        .await
        .unwrap_err();

    assert_eq!(error.code, "row_count_mismatch");
    assert_eq!(error.detail.unwrap()["operation"], 1);
    assert_eq!(names(&fixture, &session).await[0], text("Ada"));
}

#[tokio::test]
async fn refuses_what_cannot_be_edited() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let read_only = fixture.session(true).await;
    let delete = |id| OperationArg::Delete {
        key: vec![kv("id", int(id))],
    };
    let cases = [
        (args(&read_only, "customers", vec![delete(1)]), "read_only"),
        (
            args(&session, "paid_orders", vec![delete(1)]),
            "invalid_argument",
        ),
        (
            args(
                &session,
                "event_log",
                vec![OperationArg::Delete {
                    key: vec![kv("message", text("x"))],
                }],
            ),
            "invalid_argument",
        ),
        (
            args(
                &session,
                "order_items",
                vec![OperationArg::Delete {
                    key: vec![kv("order_id", int(10))],
                }],
            ),
            "invalid_argument",
        ),
        (
            args(
                &session,
                "employees",
                vec![OperationArg::Update {
                    key: vec![kv("id", int(1))],
                    set: vec![kv("full_name", text("x"))],
                }],
            ),
            "invalid_argument",
        ),
        (
            args(
                &session,
                "customers",
                vec![OperationArg::Update {
                    key: vec![kv("id", int(1))],
                    set: vec![kv(
                        "name",
                        WireValue::Text(TextPayload {
                            v: "cut".into(),
                            truncated: true,
                        }),
                    )],
                }],
            ),
            "invalid_argument",
        ),
        (
            args(
                &session,
                "customers",
                vec![OperationArg::Insert {
                    values: vec![kv("nope", int(1))],
                }],
            ),
            "invalid_argument",
        ),
    ];

    for (case, code) in cases {
        let table = case.table.name.clone();
        let error = commands::changes_apply(&fixture.state, case)
            .await
            .unwrap_err();
        assert_eq!(error.code, code, "{table}: {}", error.message);
    }
}

#[tokio::test]
async fn a_table_without_a_key_is_edited_by_rowid() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let page_args = || TablePageArgs {
        session_id: session.session_id.clone(),
        view_id: "view-2".into(),
        table: table("event_log"),
        filter: None,
        key: Vec::new(),
        sort: Vec::new(),
        limit: 50,
        offset: 0,
        after: None,
    };
    let before = commands::table_page(&fixture.state, page_args())
        .await
        .unwrap();
    assert_eq!(before.columns[0].name, "rowid");
    let first = before.rows[0][0].clone();

    let preview = commands::changes_preview(
        &fixture.state,
        args(
            &session,
            "event_log",
            vec![OperationArg::Update {
                key: vec![kv("rowid", first.clone())],
                set: vec![kv("message", text("edited"))],
            }],
        ),
    )
    .await
    .unwrap();
    commands::changes_apply(
        &fixture.state,
        args(
            &session,
            "event_log",
            vec![
                OperationArg::Update {
                    key: vec![kv("rowid", first.clone())],
                    set: vec![kv("message", text("edited"))],
                },
                OperationArg::Delete {
                    key: vec![kv("rowid", before.rows[1][0].clone())],
                },
            ],
        ),
    )
    .await
    .unwrap();
    let writing_rowid = commands::changes_preview(
        &fixture.state,
        args(
            &session,
            "event_log",
            vec![OperationArg::Update {
                key: vec![kv("rowid", first.clone())],
                set: vec![kv("rowid", int(99))],
            }],
        ),
    )
    .await
    .unwrap_err();
    let after = commands::table_page(&fixture.state, page_args())
        .await
        .unwrap();

    assert_eq!(
        preview,
        [format!(
            "UPDATE \"main\".\"event_log\" SET \"message\" = 'edited' WHERE \"rowid\" = {}",
            1
        )]
    );
    assert_eq!(after.rows.len(), before.rows.len() - 1);
    assert_eq!(after.rows[0][2], text("edited"));
    assert_eq!(writing_rowid.code, "invalid_argument");
}
