//! `schema_edit_preview` and `schema_edit_apply` (task 9.9), over a SQLite fixture.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use dbc_core::schema::{Index, NamespaceRef, TableInfo, TableRef};
use dbc_core::schema_edit::{ColumnDefinition, SchemaChange, SchemaEdit};
use dunlin_lib::commands;
use dunlin_lib::ipc::{SchemaEditArgs, SchemaTableArgs, SessionInfo};

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

async fn describe(fixture: &Fixture, session: &SessionInfo, name: &str) -> TableInfo {
    commands::schema_table(
        &fixture.state,
        SchemaTableArgs {
            session_id: session.session_id.clone(),
            table: table(name),
        },
    )
    .await
    .unwrap()
}

fn args(session: &SessionInfo, edit: SchemaEdit, confirmed: bool) -> SchemaEditArgs {
    SchemaEditArgs {
        session_id: session.session_id.clone(),
        edit,
        confirmed: Some(confirmed),
    }
}

fn add_phone() -> SchemaChange {
    SchemaChange::AddColumn {
        column: ColumnDefinition {
            name: "phone".into(),
            type_name: "TEXT".into(),
            nullable: true,
            default: None,
            comment: None,
        },
    }
}

fn drop_customer_index() -> SchemaChange {
    SchemaChange::DropIndex {
        index: Index {
            name: "orders_customer_idx".into(),
            columns: vec!["customer_id".into()],
            unique: false,
            method: None,
            predicate: None,
        },
    }
}

#[tokio::test]
async fn previews_then_applies_and_refreshes_the_cached_table() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let original = describe(&fixture, &session, "customers").await;
    let edit = SchemaEdit {
        original,
        changes: vec![add_phone()],
    };

    let preview =
        commands::schema_edit_preview(&fixture.state, args(&session, edit.clone(), false))
            .await
            .unwrap();
    assert_eq!(
        preview.statements,
        [r#"ALTER TABLE "main"."customers" ADD COLUMN "phone" TEXT"#]
    );
    assert!(!preview.destructive);
    assert!(preview.transactional);

    let applied = commands::schema_edit_apply(&fixture.state, args(&session, edit, false))
        .await
        .unwrap();
    assert_eq!(applied.statements, 1);
    let after = describe(&fixture, &session, "customers").await;
    assert!(
        after.column_named("phone").is_some(),
        "the cache was cleared"
    );
}

#[tokio::test]
async fn drops_need_confirmation() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let edit = SchemaEdit {
        original: describe(&fixture, &session, "orders").await,
        changes: vec![drop_customer_index()],
    };
    let error = commands::schema_edit_apply(&fixture.state, args(&session, edit.clone(), false))
        .await
        .unwrap_err();
    assert_eq!(error.code, "confirmation_required");
    assert!(describe(&fixture, &session, "orders").await.indexes.len() == 1);

    commands::schema_edit_apply(&fixture.state, args(&session, edit, true))
        .await
        .unwrap();
    assert!(
        describe(&fixture, &session, "orders")
            .await
            .indexes
            .is_empty()
    );
}

#[tokio::test]
async fn read_only_sessions_are_refused() {
    let fixture = Fixture::new();
    let session = fixture.session(true).await;
    let edit = SchemaEdit {
        original: describe(&fixture, &session, "customers").await,
        changes: vec![add_phone()],
    };
    let error = commands::schema_edit_apply(&fixture.state, args(&session, edit.clone(), true))
        .await
        .unwrap_err();
    assert_eq!(error.code, "read_only");
    let error = commands::schema_edit_preview(&fixture.state, args(&session, edit, true))
        .await
        .unwrap_err();
    assert_eq!(error.code, "read_only");
}

#[tokio::test]
async fn a_stale_description_is_refused() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let original = describe(&fixture, &session, "customers").await;
    fixture
        .run(&session, "ALTER TABLE customers ADD COLUMN fax TEXT")
        .await
        .unwrap();
    let edit = SchemaEdit {
        original,
        changes: vec![add_phone()],
    };
    let error = commands::schema_edit_apply(&fixture.state, args(&session, edit, true))
        .await
        .unwrap_err();
    assert!(error.message.contains("changed since"), "{error:?}");
    assert!(
        describe(&fixture, &session, "customers")
            .await
            .column_named("phone")
            .is_none()
    );
}

#[tokio::test]
async fn a_failure_names_the_statement() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    // `UNIQUE` on a column with duplicates fails when the copy is made.
    fixture
        .run_with(
            &session,
            "UPDATE customers SET name = 'same'",
            1000,
            Some(true),
        )
        .await
        .unwrap();
    let edit = SchemaEdit {
        original: describe(&fixture, &session, "customers").await,
        changes: vec![SchemaChange::AddUnique {
            key: dbc_core::schema::Key {
                name: None,
                columns: vec!["name".into()],
            },
        }],
    };
    let error = commands::schema_edit_apply(&fixture.state, args(&session, edit, true))
        .await
        .unwrap_err();
    let detail = error.detail.unwrap();
    assert_eq!(detail["statement"], 1, "{detail}");
    assert_eq!(detail["applied"], 0, "{detail}");
}

#[tokio::test]
async fn invalid_edits_are_refused_before_planning() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let edit = SchemaEdit {
        original: describe(&fixture, &session, "customers").await,
        changes: vec![SchemaChange::DropColumn {
            name: "missing".into(),
        }],
    };
    let error = commands::schema_edit_preview(&fixture.state, args(&session, edit, true))
        .await
        .unwrap_err();
    assert!(error.message.contains("no column missing"), "{error:?}");
}
