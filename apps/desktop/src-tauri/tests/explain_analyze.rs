//! `query_explain_analyze` (task 10.6), over a SQLite fixture.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use dunlin_lib::commands;
use dunlin_lib::ipc::{QueryExplainAnalyzeArgs, SessionInfo, TransactionModeArgs};

use common::Fixture;

fn args(session: &SessionInfo, sql: &str, confirmed: bool) -> QueryExplainAnalyzeArgs {
    QueryExplainAnalyzeArgs {
        session_id: session.session_id.clone(),
        tab_id: "tab-1".into(),
        sql: sql.into(),
        confirmed: Some(confirmed),
    }
}

async fn events(fixture: &Fixture, session: &SessionInfo) -> String {
    let result = fixture
        .run(session, "SELECT count(*) FROM event_log")
        .await
        .unwrap();
    format!("{:?}", result.outcome)
}

#[tokio::test]
async fn a_read_is_measured() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let report = commands::query_explain_analyze(
        &fixture.state,
        args(&session, "SELECT * FROM customers", false),
    )
    .await
    .unwrap();
    assert!(report.measured);
    assert!(report.total_ms.is_some());
    assert!(!report.plan.operation.is_empty());
}

#[tokio::test]
async fn a_write_needs_confirmation_when_dangerous_and_is_rolled_back() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let before = events(&fixture, &session).await;

    let error = commands::query_explain_analyze(
        &fixture.state,
        args(&session, "DELETE FROM event_log", false),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "confirmation_required");

    commands::query_explain_analyze(
        &fixture.state,
        args(&session, "DELETE FROM event_log", true),
    )
    .await
    .unwrap();
    assert_eq!(events(&fixture, &session).await, before);
}

#[tokio::test]
async fn read_only_sessions_measure_reads_only() {
    let fixture = Fixture::new();
    let session = fixture.session(true).await;
    commands::query_explain_analyze(&fixture.state, args(&session, "SELECT 1", false))
        .await
        .unwrap();
    let error = commands::query_explain_analyze(
        &fixture.state,
        args(&session, "DELETE FROM event_log WHERE 1 = 0", true),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "read_only");
}

#[tokio::test]
async fn a_write_is_refused_in_a_manual_transaction() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    commands::transaction_mode_set(
        &fixture.state,
        TransactionModeArgs {
            session_id: session.session_id.clone(),
            tab_id: "tab-1".into(),
            manual: true,
        },
    )
    .await
    .unwrap();
    let error = commands::query_explain_analyze(
        &fixture.state,
        args(&session, "DELETE FROM event_log WHERE 1 = 0", true),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "invalid_argument");
    assert!(error.message.contains("auto-commit"), "{error:?}");
}

#[tokio::test]
async fn only_queries_and_data_changes_are_measured() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    for sql in ["CREATE TABLE x (a int)", "BEGIN", "SELECT 1; SELECT 2", ""] {
        let error = commands::query_explain_analyze(&fixture.state, args(&session, sql, true))
            .await
            .unwrap_err();
        assert_eq!(error.code, "invalid_argument", "{sql}");
    }
}
