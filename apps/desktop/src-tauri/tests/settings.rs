//! Settings commands (task 8.8).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use dbc_store::{PreviewPolicy, Settings, Theme};
use dunlin_lib::commands;
use dunlin_lib::ipc::{HistoryListArgs, SettingsSetArgs};

use common::Fixture;

#[tokio::test]
async fn settings_survive_a_restart() {
    let fixture = Fixture::new();
    let changed = Settings {
        theme: Theme::Light,
        default_page_size: 500,
        preview_policy: PreviewPolicy::Always,
        null_text: "∅".into(),
        ..Settings::default()
    };

    let saved = commands::settings_set(
        &fixture.state,
        SettingsSetArgs {
            settings: changed.clone(),
        },
    )
    .await
    .unwrap();
    let reopened = dunlin_lib::state::AppState::open(
        &fixture.dir.path().join("store.db"),
        std::sync::Arc::new(dbc_store::MemorySecretStore::default()),
    )
    .unwrap();

    assert_eq!(saved, changed);
    assert_eq!(commands::settings_get(&fixture.state), changed);
    assert_eq!(commands::settings_get(&reopened), changed);
}

#[tokio::test]
async fn invalid_settings_are_refused_and_the_old_ones_kept() {
    let fixture = Fixture::new();
    let error = commands::settings_set(
        &fixture.state,
        SettingsSetArgs {
            settings: Settings {
                editor_font_size: 200,
                ..Settings::default()
            },
        },
    )
    .await
    .unwrap_err();

    assert_eq!(error.code, "invalid_argument");
    assert_eq!(commands::settings_get(&fixture.state), Settings::default());
}

#[tokio::test]
async fn history_follows_the_history_cap() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;
    let off = Settings {
        history_cap: 0,
        ..Settings::default()
    };
    commands::settings_set(&fixture.state, SettingsSetArgs { settings: off })
        .await
        .unwrap();
    fixture.run(&session, "SELECT 1").await.unwrap();
    let one = Settings {
        history_cap: 1,
        ..Settings::default()
    };
    commands::settings_set(&fixture.state, SettingsSetArgs { settings: one })
        .await
        .unwrap();
    fixture.run(&session, "SELECT 2").await.unwrap();
    fixture.run(&session, "SELECT 3").await.unwrap();

    let entries = commands::history_list(
        &fixture.state,
        HistoryListArgs {
            connection_id: session.connection_id.clone(),
            search: None,
            limit: 50,
            before: None,
        },
    )
    .await
    .unwrap();

    let statements: Vec<&str> = entries.iter().map(|e| e.statement.as_str()).collect();
    assert_eq!(statements, vec!["SELECT 3"]);
}
