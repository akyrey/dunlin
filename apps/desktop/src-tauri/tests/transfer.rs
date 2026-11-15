//! Exporting and importing connection profiles (deferred feature 7).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::{Fixture, draft};
use dunlin_lib::commands;
use dunlin_lib::ipc::{
    ConnectionSaveArgs, ConnectionSecrets, ConnectionsExportArgs, ConnectionsImportArgs,
};

const PASSWORD: &str = "Not-In-The-File-1f9a";

#[tokio::test]
async fn exported_connections_import_elsewhere_without_their_secrets() {
    let source = Fixture::new();
    let mut first = draft(&source.dir.path().join("a.db"));
    first.name = "Alpha".into();
    first.folder = Some("Prod".into());
    let saved = commands::connection_save(
        &source.state,
        ConnectionSaveArgs {
            profile: first,
            secrets: Some(ConnectionSecrets {
                db_password: Some(Some(PASSWORD.into())),
                ..Default::default()
            }),
        },
    )
    .await
    .unwrap();
    let mut second = draft(&source.dir.path().join("b.db"));
    second.name = "Beta".into();
    commands::connection_save(
        &source.state,
        ConnectionSaveArgs {
            profile: second,
            secrets: None,
        },
    )
    .await
    .unwrap();
    let path = source.dir.path().join("connections.json");

    let all = commands::connections_export(
        &source.state,
        ConnectionsExportArgs {
            path: path.to_string_lossy().into(),
            ids: None,
        },
    )
    .await
    .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let target = Fixture::new();
    let report = commands::connections_import(
        &target.state,
        ConnectionsImportArgs {
            path: path.to_string_lossy().into(),
        },
    )
    .await
    .unwrap();

    assert_eq!(all, 2);
    assert!(!text.contains(PASSWORD), "secrets stay in the keychain");
    assert!(!text.contains(&saved.id), "ids are not exported");
    let names: Vec<_> = report.imported.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Alpha", "Beta"]);
    assert_eq!(report.imported[0].folder.as_deref(), Some("Prod"));
    assert!(report.imported[0].saved_secrets.is_empty());
    assert!(report.skipped.is_empty());
}

#[tokio::test]
async fn invalid_entries_are_skipped_and_foreign_files_refused() {
    let fixture = Fixture::new();
    let path = fixture.dir.path().join("mixed.json");
    std::fs::write(
        &path,
        r#"{"format": "dunlin-connections", "version": 1, "connections": [
            {"id": "x", "name": "Good", "driver": "sqlite", "color": null, "host": null,
             "port": null, "database": null, "user": null, "filePath": "/tmp/good.db"},
            {"id": null, "name": "No host", "driver": "postgres", "color": null, "host": null,
             "port": 5432, "database": null, "user": null, "filePath": null}
        ]}"#,
    )
    .unwrap();
    let foreign = fixture.dir.path().join("other.json");
    std::fs::write(
        &foreign,
        r#"{"format": "something", "version": 1, "connections": []}"#,
    )
    .unwrap();
    let import = |p: &std::path::Path| ConnectionsImportArgs {
        path: p.to_string_lossy().into(),
    };

    let report = commands::connections_import(&fixture.state, import(&path))
        .await
        .unwrap();
    let refused = commands::connections_import(&fixture.state, import(&foreign))
        .await
        .unwrap_err();

    assert_eq!(report.imported.len(), 1);
    assert_ne!(
        report.imported[0].id, "x",
        "imports are always new connections"
    );
    assert_eq!(report.skipped.len(), 1);
    assert!(
        report.skipped[0].starts_with("No host: "),
        "{:?}",
        report.skipped
    );
    assert_eq!(refused.code, "invalid_argument");
}
