#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::{Fixture, draft};

use std::path::Path;

use dbc_core::WireValue;
use dbc_core::driver::DriverKind;
use dbc_core::schema::{NamespaceRef, TableRef};
use dbc_core::sql::StatementClass;
use dbc_store::ConnectionDraft;
use dunlin_lib::commands::{self, MAX_ROW_LIMIT};
use dunlin_lib::ipc::{
    ConnectionIdArgs, ConnectionSaveArgs, ConnectionTestArgs, QueryExecuteArgs, QueryOutcome,
    QueryResult, SessionIdArgs, SqlSplitArgs, StatementRange, TabArgs,
};

fn rows(result: QueryResult) -> (Vec<Vec<WireValue>>, bool) {
    match result.outcome {
        QueryOutcome::Rows {
            rows, truncated, ..
        } => (rows, truncated),
        other => panic!("expected rows, got {other:?}"),
    }
}

#[test]
fn driver_list_contains_every_driver() {
    let fixture = Fixture::new();

    let drivers = commands::driver_list(&fixture.state);

    assert_eq!(
        drivers.iter().map(|d| d.kind).collect::<Vec<_>>(),
        vec![
            DriverKind::Postgres,
            DriverKind::Mysql,
            DriverKind::Sqlite,
            DriverKind::Redis
        ]
    );
}

#[tokio::test]
async fn connections_can_be_saved_listed_and_deleted() {
    let fixture = Fixture::new();
    let saved = fixture.saved(&fixture.database(), false).await;

    let listed = commands::connection_list(&fixture.state).await.unwrap();
    let args = ConnectionIdArgs {
        connection_id: saved.id.clone(),
    };
    commands::connection_delete(&fixture.state, args)
        .await
        .unwrap();

    assert_eq!(listed, vec![saved]);
    assert!(
        commands::connection_list(&fixture.state)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn saving_without_a_file_path_is_invalid() {
    let fixture = Fixture::new();
    let profile = ConnectionDraft {
        file_path: None,
        ..draft(Path::new("x"))
    };

    let error = commands::connection_save(
        &fixture.state,
        ConnectionSaveArgs {
            profile,
            secrets: None,
        },
    )
    .await
    .err()
    .unwrap();

    assert_eq!(error.code, "invalid_argument");
}

#[tokio::test]
async fn saving_a_network_profile_without_a_host_is_invalid() {
    let fixture = Fixture::new();
    let profile = ConnectionDraft {
        driver: DriverKind::Postgres,
        file_path: None,
        ..draft(Path::new("x"))
    };

    let error = commands::connection_save(
        &fixture.state,
        ConnectionSaveArgs {
            profile,
            secrets: None,
        },
    )
    .await
    .err()
    .unwrap();

    assert_eq!(error.code, "invalid_argument");
}

#[tokio::test]
async fn testing_a_connection_reports_the_server_version() {
    let fixture = Fixture::new();
    let profile = draft(&fixture.database());

    let result = commands::connection_test(
        &fixture.state,
        ConnectionTestArgs {
            profile,
            secrets: None,
        },
    )
    .await
    .unwrap();

    assert!(result.server_version.starts_with("SQLite 3."));
}

#[tokio::test]
async fn testing_a_missing_file_fails_with_connect() {
    let fixture = Fixture::new();
    let profile = draft(&fixture.dir.path().join("absent.db"));

    let error = commands::connection_test(
        &fixture.state,
        ConnectionTestArgs {
            profile,
            secrets: None,
        },
    )
    .await
    .err()
    .unwrap();

    assert_eq!(error.code, "connect");
}

#[tokio::test]
async fn opening_an_unknown_connection_is_not_found() {
    let fixture = Fixture::new();
    let args = ConnectionIdArgs {
        connection_id: uuid::Uuid::new_v4().to_string(),
    };

    let error = commands::session_open(&fixture.state, args)
        .await
        .err()
        .unwrap();

    assert_eq!(error.code, "not_found");
}

#[tokio::test]
async fn a_select_returns_wire_values() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;

    let result = fixture
        .run(&session, "SELECT 9007199254740993, NULL, 'x'")
        .await
        .unwrap();

    assert_eq!(result.statement_class, StatementClass::Read);
    let json = serde_json::to_value(rows(result).0).unwrap();
    assert_eq!(
        json,
        serde_json::json!([[{"t": "int", "v": "9007199254740993"}, {"t": "null"}, {"t": "text", "v": "x"}]])
    );
}

#[tokio::test]
async fn results_beyond_the_row_limit_are_truncated() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let sql = "WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM s WHERE n < 50) SELECT n FROM s";

    let (rows, truncated) = rows(fixture.run_with(&session, sql, 20, None).await.unwrap());

    assert_eq!((rows.len(), truncated), (20, true));
}

#[tokio::test]
async fn writes_report_affected_rows_as_a_string() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;

    let result = fixture
        .run(&session, "UPDATE orders SET paid = 1 WHERE customer_id = 1")
        .await;

    assert_eq!(
        result.unwrap().outcome,
        QueryOutcome::Affected { rows: "2".into() }
    );
}

#[tokio::test]
async fn syntax_errors_carry_a_utf16_position() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let args = QueryExecuteArgs {
        session_id: session.session_id.clone(),
        tab_id: "t".into(),
        sql: "SELECT '🌍' FRM customers".into(),
        row_limit: 10,
        confirmed: None,
    };

    let error = commands::query_execute(&fixture.state, args)
        .await
        .err()
        .unwrap();

    // `FRM` parses as a column alias, so SQLite reports `customers`: byte 18, UTF-16 offset 16.
    assert_eq!(error.code, "syntax");
    assert_eq!(error.detail, Some(serde_json::json!({ "position": 16 })));
}

#[tokio::test]
async fn read_only_sessions_reject_writes_before_sending_them() {
    let fixture = Fixture::new();
    let session = fixture.session(true).await;

    let insert = fixture
        .run(&session, "INSERT INTO event_log (message) VALUES ('x')")
        .await;
    let select = fixture
        .run(&session, "SELECT count(*) FROM event_log")
        .await;

    assert_eq!(insert.err().as_deref(), Some("read_only"));
    assert!(select.is_ok());
}

#[tokio::test]
async fn read_only_is_also_enforced_by_the_database() {
    let fixture = Fixture::new();
    let session = fixture.session(true).await;

    // A write the classifier lets through (PRAGMA with "=" is a session statement) but that
    // SQLite itself refuses on a read-only handle.
    let result = fixture.run(&session, "PRAGMA user_version = 7").await;

    assert_eq!(result.err().as_deref(), Some("read_only"));
}

#[tokio::test]
async fn dangerous_statements_need_confirmation() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;

    let unconfirmed = fixture.run(&session, "DELETE FROM event_log").await;
    let confirmed = fixture
        .run_with(&session, "DELETE FROM event_log", 10, Some(true))
        .await;

    assert_eq!(unconfirmed.err().as_deref(), Some("confirmation_required"));
    assert_eq!(
        confirmed.unwrap().outcome,
        QueryOutcome::Affected { rows: "2".into() }
    );
}

#[tokio::test]
async fn invalid_arguments_are_rejected() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;

    let empty = fixture.run(&session, "   ").await;
    let zero = fixture.run_with(&session, "SELECT 1", 0, None).await;
    let huge = fixture
        .run_with(&session, "SELECT 1", MAX_ROW_LIMIT + 1, None)
        .await;

    assert_eq!(empty.err().as_deref(), Some("invalid_argument"));
    assert_eq!(zero.err().as_deref(), Some("invalid_argument"));
    assert_eq!(huge.err().as_deref(), Some("invalid_argument"));
}

#[tokio::test]
async fn split_returns_utf16_ranges() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let args = SqlSplitArgs {
        session_id: session.session_id.clone(),
        text: "SELECT '🌍'; DELETE FROM t".into(),
    };

    let ranges = commands::sql_split(&fixture.state, args).await.unwrap();

    assert_eq!(
        ranges,
        vec![
            StatementRange {
                start: 0,
                end: 11,
                danger: None
            },
            StatementRange {
                start: 13,
                end: 26,
                danger: Some("DELETE without WHERE affects every row".into())
            }
        ]
    );
}

#[tokio::test]
async fn tabs_have_independent_transactions() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let in_tab = |tab: &str, sql: &str| QueryExecuteArgs {
        session_id: session.session_id.clone(),
        tab_id: tab.into(),
        sql: sql.into(),
        row_limit: 10,
        confirmed: None,
    };
    let count = "SELECT count(*) FROM tags";

    commands::query_execute(&fixture.state, in_tab("a", "BEGIN"))
        .await
        .unwrap();
    commands::query_execute(
        &fixture.state,
        in_tab("a", "INSERT INTO tags (code) VALUES ('new')"),
    )
    .await
    .unwrap();
    let inside = commands::query_execute(&fixture.state, in_tab("a", count))
        .await
        .unwrap();
    let outside = commands::query_execute(&fixture.state, in_tab("b", count))
        .await
        .unwrap();

    assert_eq!(rows(inside).0, vec![vec![WireValue::Int { v: "1".into() }]]);
    assert_eq!(
        rows(outside).0,
        vec![vec![WireValue::Int { v: "0".into() }]]
    );
}

#[tokio::test]
async fn tabs_keep_their_own_session_state() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    fixture
        .run(&session, "CREATE TEMP TABLE scratch (a)")
        .await
        .unwrap();

    let other_tab = QueryExecuteArgs {
        session_id: session.session_id.clone(),
        tab_id: "tab-2".into(),
        sql: "SELECT * FROM scratch".into(),
        row_limit: 10,
        confirmed: None,
    };
    let from_other_tab = commands::query_execute(&fixture.state, other_tab).await;

    assert_eq!(
        from_other_tab.err().map(|e| e.code).as_deref(),
        Some("syntax")
    );
    assert!(fixture.run(&session, "SELECT * FROM scratch").await.is_ok());
}

#[tokio::test]
async fn closing_a_tab_drops_its_connection_state() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    fixture
        .run(&session, "CREATE TEMP TABLE scratch (a)")
        .await
        .unwrap();
    let args = TabArgs {
        session_id: session.session_id.clone(),
        tab_id: "tab-1".into(),
    };

    commands::tab_close(&fixture.state, args).await.unwrap();

    assert_eq!(
        fixture
            .run(&session, "SELECT * FROM scratch")
            .await
            .err()
            .as_deref(),
        Some("syntax")
    );
}

#[tokio::test]
async fn closed_sessions_are_gone() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let args = SessionIdArgs {
        session_id: session.session_id.clone(),
    };

    commands::session_close(&fixture.state, args).await.unwrap();

    assert_eq!(
        fixture.run(&session, "SELECT 1").await.err().as_deref(),
        Some("not_found")
    );
}

#[tokio::test]
async fn deleting_a_connection_closes_its_sessions() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let args = ConnectionIdArgs {
        connection_id: session.connection_id.clone(),
    };

    commands::connection_delete(&fixture.state, args)
        .await
        .unwrap();

    assert_eq!(fixture.state.sessions.count().await, 0);
}

#[tokio::test]
async fn several_statements_at_once_are_refused() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;

    let result = fixture
        .run(&session, "SELECT 1; DROP TABLE customers")
        .await;

    assert_eq!(result.err().as_deref(), Some("invalid_argument"));
}

#[tokio::test]
async fn cancelling_stops_a_running_statement_and_is_a_no_op_when_idle() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let tab = TabArgs {
        session_id: session.session_id.clone(),
        tab_id: "tab-1".into(),
    };
    commands::query_cancel(&fixture.state, tab.clone())
        .await
        .unwrap();
    let infinite =
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c";

    let run = fixture.run(&session, infinite);
    let cancel = async {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            commands::query_cancel(&fixture.state, tab.clone())
                .await
                .unwrap();
        }
    };
    let result = tokio::select! { r = run => r, () = cancel => unreachable!() };

    assert_eq!(result.err().as_deref(), Some("cancelled"));
    assert!(fixture.run(&session, "SELECT 1").await.is_ok());
}

#[tokio::test]
async fn read_only_sessions_reject_dml_and_ddl_before_sending() {
    let fixture = Fixture::new();
    let session = fixture.session(true).await;

    // MySQL's read-only transaction mode does not stop DDL, so the classifier must.
    for sql in [
        "INSERT INTO event_log (message) VALUES ('x')",
        "UPDATE orders SET paid = 1 WHERE id = 10",
        "DELETE FROM event_log WHERE message IS NULL",
        "CREATE TABLE t (a int)",
        "DROP TABLE customers",
        "SET default_transaction_read_only = off",
    ] {
        let result = fixture.run(&session, sql).await;
        assert_eq!(result.err().as_deref(), Some("read_only"), "{sql}");
    }
}

#[tokio::test]
async fn a_plain_single_table_select_reports_its_source_table() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let main = |name: &str| TableRef {
        namespace: NamespaceRef {
            catalog: None,
            schema: Some("main".into()),
        },
        name: name.into(),
    };

    let star = fixture
        .run(&session, "SELECT * FROM customers")
        .await
        .unwrap();
    let qualified = fixture
        .run(
            &session,
            "select id, name from \"main\".customers c where id > 0",
        )
        .await
        .unwrap();
    let counted = fixture
        .run(&session, "SELECT count(*) FROM customers")
        .await
        .unwrap();

    assert_eq!(star.source, Some(main("customers")));
    assert_eq!(qualified.source, Some(main("customers")));
    assert_eq!(counted.source, None);
}

#[tokio::test]
async fn explain_returns_a_plan_and_refuses_scripts() {
    let fixture = Fixture::new();
    let session = fixture.session(true).await;
    let args = |sql: &str| dunlin_lib::ipc::QueryExplainArgs {
        session_id: session.session_id.clone(),
        tab_id: "t".into(),
        sql: sql.into(),
    };

    // Read-only sessions can explain writes: nothing runs.
    let plan = commands::query_explain(&fixture.state, args("DELETE FROM orders WHERE id = 1"))
        .await
        .unwrap();
    let script = commands::query_explain(&fixture.state, args("SELECT 1; SELECT 2"))
        .await
        .unwrap_err();

    assert!(!plan.plan.operation.is_empty());
    assert!(!plan.measured);
    assert_eq!(plan.total_ms, None);
    assert_eq!(script.code, "invalid_argument");
}
