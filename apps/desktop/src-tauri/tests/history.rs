//! Query history commands (tasks 5.11, 5.12).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use dbc_store::HistoryOutcome;
use dunlin_lib::commands;
use dunlin_lib::ipc::{ConnectionIdArgs, HistoryListArgs};

use common::Fixture;

fn list(connection_id: &str, search: Option<&str>) -> HistoryListArgs {
    HistoryListArgs {
        connection_id: connection_id.into(),
        search: search.map(str::to_owned),
        limit: 50,
        before: None,
    }
}

#[tokio::test]
async fn every_executed_statement_is_recorded_with_its_outcome() {
    let fixture = Fixture::new();
    let path = fixture.database();
    let session = fixture.session_on(&path, false).await;
    let connection_id = session.connection_id.clone();

    fixture
        .run(&session, "SELECT * FROM customers")
        .await
        .unwrap();
    fixture
        .run(&session, "UPDATE orders SET paid = paid WHERE id = 10")
        .await
        .unwrap();
    fixture
        .run(&session, "SELECT * FRM customers")
        .await
        .unwrap_err();
    // Refused before it runs: not recorded.
    fixture
        .run(&session, "DELETE FROM event_log")
        .await
        .unwrap_err();

    let entries = commands::history_list(&fixture.state, list(&connection_id, None))
        .await
        .unwrap();
    let outcomes: Vec<(&str, HistoryOutcome, Option<u64>)> = entries
        .iter()
        .map(|e| (e.statement.as_str(), e.outcome, e.row_count))
        .collect();

    assert_eq!(
        outcomes,
        vec![
            ("SELECT * FRM customers", HistoryOutcome::Error, None),
            (
                "UPDATE orders SET paid = paid WHERE id = 10",
                HistoryOutcome::Affected,
                Some(1)
            ),
            ("SELECT * FROM customers", HistoryOutcome::Rows, Some(3)),
        ]
    );
    assert!(
        entries[0]
            .error
            .as_deref()
            .is_some_and(|e| e.contains("syntax error"))
    );
    assert!(entries.iter().all(|e| e.started_at > 0));
}

#[tokio::test]
async fn history_survives_a_restart_and_can_be_searched_and_cleared() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let connection_id = session.connection_id.clone();
    fixture
        .run(&session, "SELECT name FROM customers")
        .await
        .unwrap();
    fixture.run(&session, "SELECT 1").await.unwrap();

    let reopened = dunlin_lib::state::AppState::open(
        &fixture.dir.path().join("store.db"),
        std::sync::Arc::new(dbc_store::MemorySecretStore::default()),
    )
    .unwrap();
    let found = commands::history_list(&reopened, list(&connection_id, Some("CUSTOMERS")))
        .await
        .unwrap();
    commands::history_clear(
        &reopened,
        ConnectionIdArgs {
            connection_id: connection_id.clone(),
        },
    )
    .await
    .unwrap();
    let cleared = commands::history_list(&reopened, list(&connection_id, None))
        .await
        .unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].statement, "SELECT name FROM customers");
    assert!(cleared.is_empty());
}
