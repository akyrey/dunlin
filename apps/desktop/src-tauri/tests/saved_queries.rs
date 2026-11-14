//! Saved query commands (task 8.6).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use dbc_store::SavedQueryDraft;
use dunlin_lib::commands;
use dunlin_lib::ipc::{SavedQueryIdArgs, SavedQuerySaveArgs};

use common::Fixture;

fn save(connection_id: Option<String>, name: &str, id: Option<String>) -> SavedQuerySaveArgs {
    SavedQuerySaveArgs {
        query: SavedQueryDraft {
            id,
            connection_id,
            name: name.into(),
            sql: format!("SELECT '{name}'"),
        },
    }
}

#[tokio::test]
async fn saved_queries_survive_a_restart_and_round_trip_edits() {
    let fixture = Fixture::new();
    let profile = fixture.saved(&fixture.database(), false).await;

    let global = commands::saved_query_save(&fixture.state, save(None, "everywhere", None))
        .await
        .unwrap();
    let local =
        commands::saved_query_save(&fixture.state, save(Some(profile.id.clone()), "here", None))
            .await
            .unwrap();
    commands::saved_query_save(
        &fixture.state,
        save(Some(profile.id.clone()), "renamed", Some(local.id.clone())),
    )
    .await
    .unwrap();
    let reopened = dunlin_lib::state::AppState::open(
        &fixture.dir.path().join("store.db"),
        std::sync::Arc::new(dbc_store::MemorySecretStore::default()),
    )
    .unwrap();
    let listed = commands::saved_query_list(&reopened).await.unwrap();

    assert_eq!(
        listed
            .iter()
            .map(|q| (q.name.as_str(), q.connection_id.is_some(), q.sql.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("everywhere", false, "SELECT 'everywhere'"),
            ("renamed", true, "SELECT 'renamed'")
        ]
    );
    commands::saved_query_delete(
        &reopened,
        SavedQueryIdArgs {
            id: global.id.clone(),
        },
    )
    .await
    .unwrap();
    let again = commands::saved_query_delete(&reopened, SavedQueryIdArgs { id: global.id })
        .await
        .unwrap_err();
    assert_eq!(again.code, "not_found");
}

#[tokio::test]
async fn a_query_needs_a_name_and_an_existing_connection() {
    let fixture = Fixture::new();

    let unnamed = commands::saved_query_save(&fixture.state, save(None, " ", None))
        .await
        .unwrap_err();
    let orphan = commands::saved_query_save(&fixture.state, save(Some("nope".into()), "x", None))
        .await
        .unwrap_err();
    let missing = commands::saved_query_save(&fixture.state, save(None, "x", Some("nope".into())))
        .await
        .unwrap_err();

    assert_eq!(unnamed.code, "invalid_argument");
    assert_eq!(orphan.code, "not_found");
    assert_eq!(missing.code, "not_found");
}
