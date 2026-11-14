use std::collections::BTreeMap;
use std::path::PathBuf;

use dbc_core::driver::DriverKind;
use tempfile::TempDir;

use super::*;

fn temp_path() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    (dir, path)
}

fn sqlite_draft(name: &str) -> ConnectionDraft {
    ConnectionDraft {
        id: None,
        name: name.to_owned(),
        driver: DriverKind::Sqlite,
        color: Some("#22aa88".to_owned()),
        read_only: false,
        host: None,
        port: None,
        database: None,
        user: None,
        file_path: Some("/tmp/app.db".to_owned()),
        options: BTreeMap::from([("create_if_missing".to_owned(), "true".to_owned())]),
        tls: TlsSettings::default(),
    }
}

#[test]
fn opening_a_new_path_creates_the_latest_schema() {
    let (_dir, path) = temp_path();

    let store = Store::open(&path).unwrap();

    assert!(path.exists());
    assert_eq!(
        store.schema_version().unwrap(),
        migrations::latest_version()
    );
    assert!(store.list_connections().unwrap().is_empty());
}

#[test]
fn an_existing_empty_file_is_upgraded() {
    let (_dir, path) = temp_path();
    std::fs::write(&path, b"").unwrap();

    let store = Store::open(&path).unwrap();

    assert_eq!(
        store.schema_version().unwrap(),
        migrations::latest_version()
    );
}

#[test]
fn reopening_keeps_saved_connections() {
    let (_dir, path) = temp_path();
    let saved = Store::open(&path)
        .unwrap()
        .save_connection(sqlite_draft("local"))
        .unwrap();

    let reopened = Store::open(&path).unwrap();

    assert_eq!(reopened.list_connections().unwrap(), vec![saved]);
}

#[test]
fn a_store_from_a_newer_version_is_refused() {
    let (_dir, path) = temp_path();
    rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", 999)
        .unwrap();

    let result = Store::open(&path);

    assert!(matches!(result, Err(StoreError::TooNew { found: 999, .. })));
}

#[test]
fn saving_a_draft_without_id_creates_a_profile_with_a_uuid() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();

    let profile = store.save_connection(sqlite_draft("local")).unwrap();

    assert!(uuid::Uuid::parse_str(&profile.id).is_ok());
    assert_eq!(profile.name, "local");
    assert_eq!(profile.file_path.as_deref(), Some("/tmp/app.db"));
    assert_eq!(
        profile.options.get("create_if_missing").map(String::as_str),
        Some("true")
    );
    assert_eq!(store.get_connection(&profile.id).unwrap(), profile);
}

#[test]
fn saving_with_an_id_replaces_the_profile() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    let created = store.save_connection(sqlite_draft("before")).unwrap();
    let draft = ConnectionDraft {
        id: Some(created.id.clone()),
        read_only: true,
        ..sqlite_draft("after")
    };

    let updated = store.save_connection(draft).unwrap();

    assert_eq!(updated.id, created.id);
    assert_eq!((updated.name.as_str(), updated.read_only), ("after", true));
    assert_eq!(store.list_connections().unwrap().len(), 1);
}

#[test]
fn saving_with_an_unknown_id_is_not_found() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    let draft = ConnectionDraft {
        id: Some(uuid::Uuid::new_v4().to_string()),
        ..sqlite_draft("x")
    };

    let result = store.save_connection(draft);

    assert!(matches!(result, Err(StoreError::NotFound(_))));
}

#[test]
fn connections_are_listed_by_name_case_insensitively() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    for name in ["beta", "Alpha", "gamma"] {
        store.save_connection(sqlite_draft(name)).unwrap();
    }

    let names: Vec<String> = store
        .list_connections()
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();

    assert_eq!(names, ["Alpha", "beta", "gamma"]);
}

#[test]
fn deleting_removes_the_profile() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    let profile = store.save_connection(sqlite_draft("local")).unwrap();

    store.delete_connection(&profile.id).unwrap();

    assert!(matches!(
        store.get_connection(&profile.id),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn deleting_an_unknown_profile_is_not_found() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();

    let result = store.delete_connection("missing");

    assert!(matches!(result, Err(StoreError::NotFound(_))));
}

#[rstest::rstest]
#[case::blank_name(ConnectionDraft { name: "   ".into(), ..sqlite_draft("x") })]
#[case::long_name(ConnectionDraft { name: "n".repeat(201), ..sqlite_draft("x") })]
#[case::bad_color(ConnectionDraft { color: Some("red".into()), ..sqlite_draft("x") })]
#[case::bad_id(ConnectionDraft { id: Some("not-a-uuid".into()), ..sqlite_draft("x") })]
fn invalid_drafts_are_rejected(#[case] draft: ConnectionDraft) {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();

    let result = store.save_connection(draft);

    assert!(matches!(result, Err(StoreError::Invalid(_))));
}

#[test]
fn names_are_trimmed() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();

    let profile = store.save_connection(sqlite_draft("  local  ")).unwrap();

    assert_eq!(profile.name, "local");
}

#[test]
fn a_profile_converts_back_into_an_equivalent_draft() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    let profile = store.save_connection(sqlite_draft("local")).unwrap();

    let resaved = store
        .save_connection(ConnectionDraft::from(profile.clone()))
        .unwrap();

    assert_eq!(resaved, profile);
}

#[test]
fn tls_settings_round_trip() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    let tls = TlsSettings {
        mode: dbc_core::driver::TlsMode::VerifyFull,
        ca_file: Some("/etc/ca.pem".into()),
        cert_file: Some("/etc/client.pem".into()),
        key_file: Some("/etc/client.key".into()),
    };

    let profile = store
        .save_connection(ConnectionDraft {
            tls: tls.clone(),
            ..sqlite_draft("x")
        })
        .unwrap();

    assert_eq!(profile.tls, tls);
}

#[test]
fn the_password_flag_is_recorded_and_kept_across_edits() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    let profile = store.save_connection(sqlite_draft("x")).unwrap();

    store.set_has_db_password(&profile.id, true).unwrap();
    let edited = store
        .save_connection(ConnectionDraft {
            name: "renamed".into(),
            ..ConnectionDraft::from(profile.clone())
        })
        .unwrap();

    assert!(!profile.has_db_password);
    assert!(edited.has_db_password);
    assert!(matches!(
        store.set_has_db_password("missing", true),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn a_version_one_store_is_upgraded_in_place() {
    let (_dir, path) = temp_path();
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch(
        "CREATE TABLE connections (id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL,
           driver TEXT NOT NULL, color TEXT, read_only INTEGER NOT NULL DEFAULT 0, host TEXT,
           port INTEGER, database TEXT, username TEXT, file_path TEXT,
           options TEXT NOT NULL DEFAULT '{}', created_at INTEGER NOT NULL,
           updated_at INTEGER NOT NULL);
         INSERT INTO connections VALUES ('11111111-1111-4111-8111-111111111111', 'old', 'sqlite',
           NULL, 0, NULL, NULL, NULL, NULL, '/tmp/a.db', '{}', 0, 0);
         PRAGMA user_version = 1;",
    )
    .unwrap();
    drop(old);

    let store = Store::open(&path).unwrap();

    let profiles = store.list_connections().unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].tls, TlsSettings::default());
    assert!(!profiles[0].has_db_password);
}
