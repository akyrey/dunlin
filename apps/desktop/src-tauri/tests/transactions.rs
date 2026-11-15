//! Manual transaction mode on a real SQLite database (deferred feature 2).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::Fixture;
use dunlin_lib::commands;
use dunlin_lib::ipc::{
    QueryExecuteArgs, QueryOutcome, SessionInfo, TabArgs, TransactionFinishArgs,
    TransactionModeArgs,
};

async fn run(fixture: &Fixture, session: &SessionInfo, tab: &str, sql: &str) -> QueryOutcome {
    let args = QueryExecuteArgs {
        session_id: session.session_id.clone(),
        tab_id: tab.into(),
        sql: sql.into(),
        row_limit: 100,
        confirmed: None,
    };
    commands::query_execute(&fixture.state, args)
        .await
        .unwrap()
        .outcome
}

async fn count(fixture: &Fixture, session: &SessionInfo, tab: &str) -> usize {
    match run(
        fixture,
        session,
        tab,
        "SELECT id FROM customers WHERE name = 'tx'",
    )
    .await
    {
        QueryOutcome::Rows { rows, .. } => rows.len(),
        QueryOutcome::Affected { .. } => panic!("expected rows"),
    }
}

async fn manual(fixture: &Fixture, session: &SessionInfo, tab: &str, on: bool) {
    let args = TransactionModeArgs {
        session_id: session.session_id.clone(),
        tab_id: tab.into(),
        manual: on,
    };
    commands::transaction_mode_set(&fixture.state, args)
        .await
        .unwrap();
}

async fn finish(fixture: &Fixture, session: &SessionInfo, tab: &str, commit: bool) {
    let args = TransactionFinishArgs {
        session_id: session.session_id.clone(),
        tab_id: tab.into(),
        commit,
    };
    let state = commands::transaction_finish(&fixture.state, args)
        .await
        .unwrap();
    assert!(!state.open);
}

#[tokio::test]
async fn rolled_back_rows_are_gone_and_committed_rows_are_seen_by_other_tabs() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    manual(&fixture, &session, "a", true).await;

    run(
        &fixture,
        &session,
        "a",
        "INSERT INTO customers (id, name) VALUES (900, 'tx')",
    )
    .await;
    let status = TabArgs {
        session_id: session.session_id.clone(),
        tab_id: "a".into(),
    };
    assert!(
        commands::transaction_status(&fixture.state, status.clone())
            .await
            .unwrap()
            .open
    );
    assert_eq!(count(&fixture, &session, "a").await, 1);
    finish(&fixture, &session, "a", false).await;
    assert_eq!(count(&fixture, &session, "b").await, 0);

    run(
        &fixture,
        &session,
        "a",
        "INSERT INTO customers (id, name) VALUES (901, 'tx')",
    )
    .await;
    finish(&fixture, &session, "a", true).await;
    manual(&fixture, &session, "a", false).await;

    assert_eq!(count(&fixture, &session, "b").await, 1);
    assert!(
        !commands::transaction_status(&fixture.state, status)
            .await
            .unwrap()
            .manual
    );
}

#[tokio::test]
async fn leaving_manual_mode_with_an_open_transaction_is_refused() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    manual(&fixture, &session, "a", true).await;
    run(&fixture, &session, "a", "SELECT 1").await;

    let args = TransactionModeArgs {
        session_id: session.session_id.clone(),
        tab_id: "a".into(),
        manual: false,
    };
    let error = commands::transaction_mode_set(&fixture.state, args)
        .await
        .unwrap_err();

    assert_eq!(error.code, "invalid_argument");
}
