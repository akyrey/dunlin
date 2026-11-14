//! Task 8.16: no input from the webview can panic the backend. Random JSON is fed to every
//! command's argument type, and mutated versions of valid calls are run against a real SQLite
//! session: each must deserialise or be refused, and every command must answer `Ok` or an
//! `AppError` with a message.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use dbc_core::export::{CsvOptions, ExportFormat};
use dunlin_lib::commands;
use dunlin_lib::error::AppError;
use dunlin_lib::ipc::*;
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use serde::de::DeserializeOwned;
use serde_json::{Value as Json, json};

use common::Fixture;

/// Field names the argument types use, so random objects reach past the top level.
const KEYS: &[&str] = &[
    "sessionId",
    "tabId",
    "viewId",
    "sql",
    "rowLimit",
    "confirmed",
    "table",
    "namespace",
    "catalog",
    "schema",
    "name",
    "filter",
    "key",
    "sort",
    "column",
    "descending",
    "limit",
    "offset",
    "exact",
    "operations",
    "kind",
    "values",
    "set",
    "value",
    "t",
    "v",
    "len",
    "truncated",
    "format",
    "delimiter",
    "header",
    "null",
    "batch",
    "path",
    "columns",
    "rows",
    "text",
    "settings",
    "theme",
    "order",
    "query",
    "id",
    "connectionId",
    "search",
    "before",
    "host",
    "port",
    "algorithm",
    "fingerprint",
    "profile",
    "secrets",
    "dbPassword",
];

fn leaf() -> impl Strategy<Value = Json> {
    prop_oneof![
        Just(Json::Null),
        any::<bool>().prop_map(Json::from),
        any::<i64>().prop_map(Json::from),
        any::<u64>().prop_map(Json::from),
        any::<f64>().prop_map(|f| serde_json::Number::from_f64(f).map_or(Json::Null, Json::Number)),
        Just(json!(u64::MAX)),
        Just(json!(-1)),
        Just(json!("")),
        Just(json!("main")),
        Just(json!("people")),
        Just(json!("\u{0}\u{202e}'\"`;--")),
        "\\PC{0,40}".prop_map(Json::from),
        Just(Json::from("x".repeat(70_000))),
    ]
}

fn arb_json() -> impl Strategy<Value = Json> {
    leaf().prop_recursive(4, 48, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(Json::Array),
            prop::collection::vec((prop::sample::select(KEYS), inner), 0..8).prop_map(|pairs| {
                Json::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
            }),
        ]
    })
}

/// Replaces one value somewhere in `json` (chosen by `pick`) with `replacement`.
fn mutate(json: &Json, pick: usize, replacement: &Json) -> Json {
    fn count(json: &Json) -> usize {
        1 + match json {
            Json::Array(items) => items.iter().map(count).sum(),
            Json::Object(map) => map.values().map(count).sum(),
            _ => 0,
        }
    }
    fn walk(json: &Json, target: &mut usize, replacement: &Json) -> Json {
        if *target == 0 {
            *target = usize::MAX;
            return replacement.clone();
        }
        *target = target.saturating_sub(1);
        match json {
            Json::Array(items) => {
                Json::Array(items.iter().map(|i| walk(i, target, replacement)).collect())
            }
            Json::Object(map) => Json::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), walk(v, target, replacement)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
    let mut target = pick % count(json);
    walk(json, &mut target, replacement)
}

fn parses<T: DeserializeOwned>(json: &Json) {
    // Ok or Err are both fine; a panic fails the test.
    let _ = serde_json::from_value::<T>(json.clone());
}

proptest! {
    #![proptest_config(Config { cases: 512, ..Config::default() })]

    #[test]
    fn no_argument_type_panics_on_any_json(json in arb_json()) {
        parses::<ConnectionSaveArgs>(&json);
        parses::<ConnectionTestArgs>(&json);
        parses::<ConnectionIdArgs>(&json);
        parses::<ConnectionReorderArgs>(&json);
        parses::<SshTrustHostArgs>(&json);
        parses::<SshKnownHostRemoveArgs>(&json);
        parses::<SessionIdArgs>(&json);
        parses::<HistoryListArgs>(&json);
        parses::<TablePageArgs>(&json);
        parses::<TableCountArgs>(&json);
        parses::<ChangesArgs>(&json);
        parses::<SettingsSetArgs>(&json);
        parses::<SavedQuerySaveArgs>(&json);
        parses::<SavedQueryIdArgs>(&json);
        parses::<ExportResultArgs>(&json);
        parses::<ExportTextArgs>(&json);
        parses::<ExportTableArgs>(&json);
        parses::<CellFullValueArgs>(&json);
        parses::<SchemaObjectsArgs>(&json);
        parses::<SchemaTableArgs>(&json);
        parses::<SqlSplitArgs>(&json);
        parses::<QueryExecuteArgs>(&json);
        parses::<TabArgs>(&json);
    }
}

/// Whether the command succeeded; an error must carry a code and a message.
fn answered<T>(result: Result<T, AppError>) -> bool {
    match result {
        Ok(_) => true,
        Err(error) => {
            assert!(
                !error.code.is_empty() && !error.message.is_empty(),
                "{error:?}"
            );
            false
        }
    }
}

/// Valid calls on the fixture session, which mutations start from.
fn samples(session_id: &str) -> Vec<(&'static str, Json)> {
    let table = json!({ "namespace": { "catalog": null, "schema": "main" }, "name": "customers" });
    let key = json!([{ "column": "id", "value": { "t": "int", "v": "1" } }]);
    let columns =
        json!([{ "name": "id", "typeName": "INTEGER", "category": "integer", "nullable": false }]);
    vec![
        (
            "query",
            json!({ "sessionId": session_id, "tabId": "t", "sql": "SELECT 1", "rowLimit": 10 }),
        ),
        (
            "split",
            json!({ "sessionId": session_id, "text": "SELECT 1; SELECT 2" }),
        ),
        (
            "page",
            json!({ "sessionId": session_id, "viewId": "v", "table": table, "key": key,
            "sort": [{ "column": "id", "descending": true }], "limit": 5, "offset": 0, "filter": "id > 0" }),
        ),
        (
            "count",
            json!({ "sessionId": session_id, "viewId": "v", "table": table, "exact": true }),
        ),
        (
            "cell",
            json!({ "sessionId": session_id, "viewId": "v", "table": table, "key": key, "column": "name" }),
        ),
        (
            "changes",
            json!({ "sessionId": session_id, "viewId": "v", "table": table, "operations": [
            { "kind": "update", "key": key, "set": [{ "column": "name", "value": { "t": "text", "v": "x" } }] },
            { "kind": "insert", "values": [] },
            { "kind": "delete", "key": key },
        ] }),
        ),
        (
            "text",
            json!({ "sessionId": session_id, "columns": columns, "rows": [[{ "t": "int", "v": "1" }]],
            "format": { "kind": "csv", "delimiter": ",", "header": true, "null": "" } }),
        ),
        ("schema", json!({ "sessionId": session_id, "table": table })),
        (
            "trust",
            json!({ "host": "h", "port": 22, "algorithm": "ssh-ed25519", "fingerprint": "SHA256:x" }),
        ),
        (
            "settings",
            json!({ "settings": { "theme": "dark", "editorFontSize": 14 } }),
        ),
        (
            "saved",
            json!({ "query": { "id": null, "connectionId": null, "name": "q", "sql": "SELECT 1" } }),
        ),
        ("history", json!({ "connectionId": "c", "limit": 5 })),
    ]
}

/// `None` when the arguments did not deserialise, else whether the command succeeded.
async fn run(fixture: &Fixture, which: &str, json: Json) -> Option<bool> {
    let state = &fixture.state;
    macro_rules! call {
        ($ty:ty, $command:path) => {
            match serde_json::from_value::<$ty>(json) {
                Ok(args) => Some(answered($command(state, args).await)),
                Err(error) => {
                    // Kept for a failing baseline: says which field the sample got wrong.
                    eprintln!("{which}: {error}");
                    None
                }
            }
        };
    }
    match which {
        "query" => call!(QueryExecuteArgs, commands::query_execute),
        "split" => call!(SqlSplitArgs, commands::sql_split),
        "page" => call!(TablePageArgs, commands::table_page),
        "count" => call!(TableCountArgs, commands::table_count),
        "cell" => call!(CellFullValueArgs, commands::cell_full_value),
        "changes" => call!(ChangesArgs, commands::changes_preview),
        "text" => call!(ExportTextArgs, commands::export_text),
        "schema" => call!(SchemaTableArgs, commands::schema_table),
        "trust" => call!(SshTrustHostArgs, commands::ssh_trust_host),
        "settings" => call!(SettingsSetArgs, commands::settings_set),
        "saved" => call!(SavedQuerySaveArgs, commands::saved_query_save),
        "history" => call!(HistoryListArgs, commands::history_list),
        other => panic!("no sample {other}"),
    }
}

#[test]
fn mutated_calls_are_answered_never_panicked() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let fixture = Fixture::new();
    let session = runtime.block_on(fixture.session(false));
    let samples = samples(&session.session_id);
    // Every sample is valid before mutation.
    for (which, json) in &samples {
        let outcome = runtime.block_on(run(&fixture, which, json.clone()));
        assert_eq!(outcome, Some(true), "sample {which}");
    }
    let mut runner = TestRunner::new(Config {
        cases: 384,
        ..Config::default()
    });
    let strategy = (0..samples.len(), any::<usize>(), arb_json());
    runner
        .run(&strategy, |(index, pick, replacement)| {
            let (which, json) = &samples[index];
            runtime.block_on(run(&fixture, which, mutate(json, pick, &replacement)));
            Ok(())
        })
        .unwrap();
}

#[test]
fn a_page_offset_beyond_sql_and_a_malformed_host_key_are_refused() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let fixture = Fixture::new();
    let session = runtime.block_on(fixture.session(false));
    let page = serde_json::from_value::<TablePageArgs>(json!({
        "sessionId": session.session_id, "viewId": "v",
        "table": { "namespace": { "catalog": null, "schema": "main" }, "name": "customers" },
        "limit": 5, "offset": u64::MAX,
    }))
    .unwrap();
    let key = |host: &str, fingerprint: &str| SshTrustHostArgs {
        host: host.into(),
        port: 22,
        algorithm: "ssh-ed25519".into(),
        fingerprint: fingerprint.into(),
    };

    let offset = runtime
        .block_on(commands::table_page(&fixture.state, page))
        .unwrap_err();
    let no_host = runtime
        .block_on(commands::ssh_trust_host(
            &fixture.state,
            key(" ", "SHA256:abc"),
        ))
        .unwrap_err();
    let not_sha = runtime
        .block_on(commands::ssh_trust_host(&fixture.state, key("h", "MD5:ab")))
        .unwrap_err();

    assert_eq!(offset.code, "invalid_argument");
    assert_eq!(no_host.code, "invalid_argument");
    assert_eq!(not_sha.code, "invalid_argument");
}

#[test]
fn csv_options_that_would_break_the_file_are_refused() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let fixture = Fixture::new();
    let session = runtime.block_on(fixture.session(false));
    let format = |delimiter: char, null: &str| {
        ExportFormat::Csv(CsvOptions {
            delimiter,
            header: true,
            null: null.into(),
        })
    };
    let text = |format: ExportFormat| ExportTextArgs {
        session_id: session.session_id.clone(),
        columns: Vec::new(),
        rows: Vec::new(),
        format,
    };

    for (delimiter, null) in [('"', ""), ('\n', ""), ('a', ""), (',', "a,b"), (',', "\"")] {
        let error = runtime
            .block_on(commands::export_text(
                &fixture.state,
                text(format(delimiter, null)),
            ))
            .unwrap_err();
        assert_eq!(error.code, "invalid_argument", "{delimiter:?} {null:?}");
    }
    for delimiter in [',', ';', '\t', '|'] {
        runtime
            .block_on(commands::export_text(
                &fixture.state,
                text(format(delimiter, "NULL")),
            ))
            .unwrap();
    }
}
