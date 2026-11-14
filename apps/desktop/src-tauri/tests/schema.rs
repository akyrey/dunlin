//! `schema_*` commands over a SQLite fixture (task 4.7).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::path::Path;
use std::time::Duration;

use dbc_core::schema::{NamespaceRef, ObjectKind, TableRef};
use dunlin_lib::commands;
use dunlin_lib::ipc::{
    QueryExecuteArgs, SchemaObjectsArgs, SchemaTableArgs, SessionIdArgs, SessionInfo,
};

use common::Fixture;

fn main_schema() -> NamespaceRef {
    NamespaceRef {
        catalog: None,
        schema: Some("main".into()),
    }
}

fn objects_args(session: &SessionInfo) -> SchemaObjectsArgs {
    SchemaObjectsArgs {
        session_id: session.session_id.clone(),
        namespace: main_schema(),
    }
}

fn table_args(session: &SessionInfo, name: &str) -> SchemaTableArgs {
    SchemaTableArgs {
        session_id: session.session_id.clone(),
        table: TableRef {
            namespace: main_schema(),
            name: name.into(),
        },
    }
}

async fn object_names(fixture: &Fixture, session: &SessionInfo) -> Vec<String> {
    commands::schema_objects(&fixture.state, objects_args(session))
        .await
        .unwrap()
        .into_iter()
        .map(|o| o.name)
        .collect()
}

/// Changes the schema behind the session's back, as another client would.
fn create_elsewhere(path: &Path, sql: &str) {
    rusqlite::Connection::open(path)
        .unwrap()
        .execute_batch(sql)
        .unwrap();
}

#[tokio::test]
async fn browses_namespaces_objects_tables_and_ddl() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let id = SessionIdArgs {
        session_id: session.session_id.clone(),
    };

    let namespaces = commands::schema_namespaces(&fixture.state, id)
        .await
        .unwrap();
    let objects = commands::schema_objects(&fixture.state, objects_args(&session))
        .await
        .unwrap();
    let info = commands::schema_table(&fixture.state, table_args(&session, "order_items"))
        .await
        .unwrap();
    let ddl = commands::schema_table_ddl(&fixture.state, table_args(&session, "tags"))
        .await
        .unwrap();

    assert_eq!(namespaces[0].namespace, main_schema());
    assert!(
        objects
            .iter()
            .any(|o| o.name == "paid_orders" && o.kind == ObjectKind::View)
    );
    assert_eq!(
        info.row_key,
        Some(vec!["order_id".to_owned(), "line".to_owned()])
    );
    assert!(ddl.starts_with("CREATE TABLE tags"));
}

#[tokio::test]
async fn results_are_cached_until_refresh() {
    let fixture = Fixture::new();
    let path = fixture.database();
    let session = fixture.session_on(&path, false).await;
    let before = object_names(&fixture, &session).await;

    create_elsewhere(&path, "CREATE TABLE added_elsewhere (a INTEGER)");
    let cached = object_names(&fixture, &session).await;
    commands::schema_refresh(
        &fixture.state,
        SessionIdArgs {
            session_id: session.session_id.clone(),
        },
    )
    .await
    .unwrap();
    let refreshed = object_names(&fixture, &session).await;

    assert_eq!(cached, before);
    assert!(refreshed.contains(&"added_elsewhere".to_owned()));
}

#[tokio::test]
async fn ddl_in_the_session_invalidates_the_cache() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let before = commands::schema_table(&fixture.state, table_args(&session, "tags"))
        .await
        .unwrap();

    fixture
        .run(&session, "ALTER TABLE tags ADD COLUMN colour TEXT")
        .await
        .unwrap();
    let after = commands::schema_table(&fixture.state, table_args(&session, "tags"))
        .await
        .unwrap();
    fixture
        .run(&session, "CREATE TABLE made_here (a INTEGER)")
        .await
        .unwrap();
    let names = object_names(&fixture, &session).await;

    assert_eq!(before.columns.len() + 1, after.columns.len());
    assert!(names.contains(&"made_here".to_owned()));
}

#[tokio::test]
async fn reads_do_not_invalidate_the_cache() {
    let fixture = Fixture::new();
    let path = fixture.database();
    let session = fixture.session_on(&path, false).await;
    let before = object_names(&fixture, &session).await;

    create_elsewhere(&path, "CREATE TABLE added_elsewhere (a INTEGER)");
    fixture
        .run(&session, "SELECT * FROM customers")
        .await
        .unwrap();
    fixture
        .run(&session, "INSERT INTO tags (code) VALUES ('x')")
        .await
        .unwrap();

    assert_eq!(object_names(&fixture, &session).await, before);
}

#[tokio::test]
async fn missing_tables_and_sessions_are_not_found() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;

    let table = commands::schema_table(&fixture.state, table_args(&session, "nope"))
        .await
        .unwrap_err();
    let unknown = SessionInfo {
        session_id: "nope".into(),
        ..session.clone()
    };
    let no_session = commands::schema_objects(&fixture.state, objects_args(&unknown))
        .await
        .unwrap_err();

    assert_eq!(table.code, "not_found");
    assert_eq!(no_session.code, "not_found");
}

#[tokio::test]
async fn a_running_statement_does_not_block_the_tree() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let state = &fixture.state;
    let slow = commands::query_execute(
        state,
        QueryExecuteArgs {
            session_id: session.session_id.clone(),
            tab_id: "slow".into(),
            sql: "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c"
                .into(),
            row_limit: 10,
            confirmed: None,
        },
    );
    let browse = async {
        // Let the slow statement start first.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let info = commands::schema_table(state, table_args(&session, "employees")).await;
        let cancel = commands::query_cancel(
            state,
            dunlin_lib::ipc::TabArgs {
                session_id: session.session_id.clone(),
                tab_id: "slow".into(),
            },
        )
        .await;
        (info, cancel)
    };

    let (slow, (info, cancel)) =
        tokio::time::timeout(Duration::from_secs(5), async { tokio::join!(slow, browse) })
            .await
            .expect("the tree waited for the editor");

    assert!(info.is_ok());
    assert!(cancel.is_ok());
    assert_eq!(slow.unwrap_err().code, "cancelled");
}
