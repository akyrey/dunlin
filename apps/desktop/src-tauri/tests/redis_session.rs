//! A Redis connection through the app's commands: console, splitting and safeguards (D-032).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::Fixture;
use dbc_core::driver::DriverKind;
use dbc_store::{ConnectionDraft, TlsSettings};
use dunlin_lib::commands;
use dunlin_lib::ipc::{
    ConnectionIdArgs, ConnectionSaveArgs, KvEditArgs, KvReadArgs, KvScanArgs, QueryExecuteArgs,
    QueryOutcome, SessionInfo, SqlSplitArgs, TransactionModeArgs,
};
use testcontainers_modules::redis::Redis;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

async fn open(fixture: &Fixture, port: u16, read_only: bool) -> SessionInfo {
    let profile = ConnectionDraft {
        id: None,
        name: "cache".into(),
        driver: DriverKind::Redis,
        color: None,
        read_only,
        host: Some("127.0.0.1".into()),
        port: Some(port),
        database: Some("1".into()),
        user: None,
        file_path: None,
        options: Default::default(),
        tls: TlsSettings::default(),
        ssh: None,
        always_preview: false,
        folder: None,
    };
    let saved = commands::connection_save(
        &fixture.state,
        ConnectionSaveArgs {
            profile,
            secrets: None,
        },
    )
    .await
    .unwrap();
    commands::session_open(
        &fixture.state,
        ConnectionIdArgs {
            connection_id: saved.id,
        },
    )
    .await
    .unwrap()
}

async fn run(
    fixture: &Fixture,
    session: &SessionInfo,
    sql: &str,
    confirmed: Option<bool>,
) -> Result<QueryOutcome, String> {
    commands::query_execute(
        &fixture.state,
        QueryExecuteArgs {
            session_id: session.session_id.clone(),
            tab_id: "t".into(),
            sql: sql.into(),
            row_limit: 100,
            confirmed,
        },
    )
    .await
    .map(|r| r.outcome)
    .map_err(|e| e.code)
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn the_console_splits_by_line_and_guards_writes() {
    let server = Redis::default().with_tag("7.4").start().await.unwrap();
    let port = server.get_host_port_ipv4(6379).await.unwrap();
    let fixture = Fixture::new();
    let session = open(&fixture, port, false).await;
    let read_only = open(&fixture, port, true).await;

    assert!(session.capabilities.key_value);
    let ranges = commands::sql_split(
        &fixture.state,
        SqlSplitArgs {
            session_id: session.session_id.clone(),
            text: "SET a 1\nFLUSHDB\n# done".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(ranges.len(), 2);
    assert!(ranges[1].danger.is_some());

    run(&fixture, &session, "SET greeting \"hello world\"", None)
        .await
        .unwrap();
    let QueryOutcome::Rows { rows, .. } = run(&fixture, &read_only, "GET greeting", None)
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(
        run(&fixture, &read_only, "SET greeting bye", None)
            .await
            .unwrap_err(),
        "read_only"
    );
    assert_eq!(
        run(&fixture, &session, "FLUSHDB", None).await.unwrap_err(),
        "confirmation_required"
    );
    run(&fixture, &session, "FLUSHDB", Some(true))
        .await
        .unwrap();
    let manual = commands::transaction_mode_set(
        &fixture.state,
        TransactionModeArgs {
            session_id: session.session_id.clone(),
            tab_id: "t".into(),
            manual: true,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(manual.code, "invalid_argument");
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn the_key_browser_lists_and_reads_keys() {
    let server = Redis::default().with_tag("7.4").start().await.unwrap();
    let port = server.get_host_port_ipv4(6379).await.unwrap();
    let fixture = Fixture::new();
    let session = open(&fixture, port, true).await;
    let writer = open(&fixture, port, false).await;
    run(&fixture, &writer, "HSET user:1 name Ada", None)
        .await
        .unwrap();
    run(&fixture, &writer, "SET counter 3", None).await.unwrap();

    let page = commands::kv_scan(
        &fixture.state,
        KvScanArgs {
            session_id: session.session_id.clone(),
            pattern: Some("user:*".into()),
            cursor: None,
            count: None,
        },
    )
    .await
    .unwrap();
    let content = commands::kv_read(
        &fixture.state,
        KvReadArgs {
            session_id: session.session_id.clone(),
            key: page.keys[0].key.id.clone(),
            limit: None,
        },
    )
    .await
    .unwrap();
    let bad = commands::kv_read(
        &fixture.state,
        KvReadArgs {
            session_id: session.session_id.clone(),
            key: "not base64!".into(),
            limit: None,
        },
    )
    .await
    .unwrap_err();

    assert_eq!(page.keys.len(), 1);
    assert_eq!(page.keys[0].key_type, "hash");
    assert_eq!(
        content
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["field", "value"]
    );
    assert_eq!(content.length, 1);
    assert_eq!(bad.code, "invalid_argument");
}

#[tokio::test]
async fn sql_connections_have_no_key_browser() {
    let fixture = Fixture::new();
    let session = fixture.session(false).await;

    let error = commands::kv_scan(
        &fixture.state,
        KvScanArgs {
            session_id: session.session_id.clone(),
            pattern: None,
            cursor: None,
            count: None,
        },
    )
    .await
    .unwrap_err();

    assert_eq!(error.code, "unsupported");
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn edits_are_previewed_then_run_and_read_only_connections_only_preview() {
    use dbc_core::kv::{KeyEdit, KeyName};

    let server = Redis::default().with_tag("7.4").start().await.unwrap();
    let port = server.get_host_port_ipv4(6379).await.unwrap();
    let fixture = Fixture::new();
    let session = open(&fixture, port, false).await;
    let read_only = open(&fixture, port, true).await;
    let key = KeyName::from_bytes(b"user:9").id;
    let edit = |session: &SessionInfo, preview: bool| KvEditArgs {
        session_id: session.session_id.clone(),
        key: key.clone(),
        edit: KeyEdit::HashSet {
            field: "name".into(),
            value: "Grace Hopper".into(),
        },
        preview,
    };

    let previewed = commands::kv_edit(&fixture.state, edit(&read_only, true))
        .await
        .unwrap();
    let refused = commands::kv_edit(&fixture.state, edit(&read_only, false))
        .await
        .unwrap_err();
    let ran = commands::kv_edit(&fixture.state, edit(&session, false))
        .await
        .unwrap();
    let content = commands::kv_read(
        &fixture.state,
        KvReadArgs {
            session_id: session.session_id.clone(),
            key: key.clone(),
            limit: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(previewed, [r#"HSET user:9 name "Grace Hopper""#]);
    assert_eq!(refused.code, "read_only");
    assert_eq!(ran, previewed);
    assert_eq!(content.length, 1);
}
