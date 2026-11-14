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
        ssh: None,
        always_preview: false,
        folder: None,
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
fn connections_are_listed_in_the_order_they_were_added() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    for name in ["beta", "Alpha", "gamma"] {
        store.save_connection(sqlite_draft(name)).unwrap();
    }

    assert_eq!(names(&store), ["beta", "Alpha", "gamma"]);
}

fn names(store: &Store) -> Vec<String> {
    store
        .list_connections()
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect()
}

#[test]
fn an_older_store_lists_its_connections_by_name_after_the_upgrade() {
    let (_dir, path) = temp_path();
    {
        let store = Store::open(&path).unwrap();
        for name in ["beta", "Alpha", "gamma"] {
            store.save_connection(sqlite_draft(name)).unwrap();
        }
    }
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch(
        "ALTER TABLE connections DROP COLUMN folder;
         ALTER TABLE connections DROP COLUMN position;
         PRAGMA user_version = 7;",
    )
    .unwrap();
    drop(raw);

    let store = Store::open(&path).unwrap();

    assert_eq!(names(&store), ["Alpha", "beta", "gamma"]);
}

#[test]
fn connections_can_be_reordered_only_as_a_whole() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    let ids: Vec<String> = ["a", "b", "c"]
        .into_iter()
        .map(|n| store.save_connection(sqlite_draft(n)).unwrap().id)
        .collect();

    store
        .reorder_connections(&[ids[2].clone(), ids[0].clone(), ids[1].clone()])
        .unwrap();
    let missing = store.reorder_connections(&[ids[0].clone(), ids[1].clone()]);
    let twice = store.reorder_connections(&[ids[0].clone(), ids[0].clone(), ids[1].clone()]);
    let unknown = store.reorder_connections(&[
        ids[0].clone(),
        ids[1].clone(),
        uuid::Uuid::new_v4().to_string(),
    ]);

    assert_eq!(names(&store), ["c", "a", "b"]);
    assert!(matches!(missing, Err(StoreError::Invalid(_))));
    assert!(matches!(twice, Err(StoreError::Invalid(_))));
    assert!(matches!(unknown, Err(StoreError::NotFound(_))));
}

#[test]
fn a_duplicate_follows_its_original_without_its_secrets() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    let first = store.save_connection(sqlite_draft("first")).unwrap();
    store.save_connection(sqlite_draft("second")).unwrap();
    store
        .set_secret_saved(&first.id, SecretKind::DbPassword, true)
        .unwrap();

    let copy = store.duplicate_connection(&first.id).unwrap();

    assert_eq!(names(&store), ["first", "first copy", "second"]);
    assert_ne!(copy.id, first.id);
    assert_eq!(copy.file_path, first.file_path);
    assert!(copy.saved_secrets.is_empty());
}

#[test]
fn folders_are_trimmed_and_empty_means_none() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();

    let filed = store
        .save_connection(ConnectionDraft {
            folder: Some("  Production ".into()),
            ..sqlite_draft("prod")
        })
        .unwrap();
    let blank = store
        .save_connection(ConnectionDraft {
            folder: Some("   ".into()),
            ..sqlite_draft("local")
        })
        .unwrap();
    let too_long = store.save_connection(ConnectionDraft {
        folder: Some("f".repeat(201)),
        ..sqlite_draft("x")
    });

    assert_eq!(filed.folder.as_deref(), Some("Production"));
    assert_eq!(blank.folder, None);
    assert!(matches!(too_long, Err(StoreError::Invalid(_))));
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
fn saved_secret_kinds_are_recorded_and_kept_across_edits() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    let profile = store.save_connection(sqlite_draft("x")).unwrap();

    store
        .set_secret_saved(&profile.id, SecretKind::SshPassword, true)
        .unwrap();
    store
        .set_secret_saved(&profile.id, SecretKind::DbPassword, true)
        .unwrap();
    store
        .set_secret_saved(&profile.id, SecretKind::DbPassword, true)
        .unwrap();
    let edited = store
        .save_connection(ConnectionDraft {
            name: "renamed".into(),
            ..ConnectionDraft::from(profile.clone())
        })
        .unwrap();
    store
        .set_secret_saved(&profile.id, SecretKind::SshPassword, false)
        .unwrap();

    assert!(profile.saved_secrets.is_empty());
    assert_eq!(
        edited.saved_secrets,
        vec![SecretKind::DbPassword, SecretKind::SshPassword]
    );
    assert_eq!(
        store.get_connection(&profile.id).unwrap().saved_secrets,
        vec![SecretKind::DbPassword]
    );
    assert!(edited.has_secret(SecretKind::DbPassword));
    assert!(matches!(
        store.set_secret_saved("missing", SecretKind::DbPassword, true),
        Err(StoreError::NotFound(_))
    ));
}

fn ssh_host(host: &str, auth: SshAuthMethod, key_file: Option<&str>) -> SshHostSettings {
    SshHostSettings {
        host: host.into(),
        port: 22,
        user: "deploy".into(),
        auth,
        key_file: key_file.map(str::to_owned),
    }
}

#[test]
fn ssh_settings_round_trip() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    let ssh = SshSettings {
        server: ssh_host(
            "bastion.internal",
            SshAuthMethod::KeyFile,
            Some("/home/me/.ssh/id_ed25519"),
        ),
        jump: Some(ssh_host("jump.example.com", SshAuthMethod::Agent, None)),
    };

    let profile = store
        .save_connection(ConnectionDraft {
            ssh: Some(ssh.clone()),
            ..sqlite_draft("x")
        })
        .unwrap();

    assert_eq!(profile.ssh, Some(ssh));
}

#[rstest::rstest]
#[case::no_host(ssh_host(" ", SshAuthMethod::Agent, None))]
#[case::no_user(SshHostSettings { user: String::new(), ..ssh_host("h", SshAuthMethod::Agent, None) })]
#[case::no_port(SshHostSettings { port: 0, ..ssh_host("h", SshAuthMethod::Agent, None) })]
#[case::key_without_file(ssh_host("h", SshAuthMethod::KeyFile, None))]
fn invalid_ssh_settings_are_rejected(#[case] server: SshHostSettings) {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    let draft = ConnectionDraft {
        ssh: Some(SshSettings { server, jump: None }),
        ..sqlite_draft("x")
    };

    assert!(matches!(
        store.save_connection(draft),
        Err(StoreError::Invalid(_))
    ));
}

#[test]
fn a_version_two_password_flag_becomes_a_saved_secret() {
    let (_dir, path) = temp_path();
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch(
        "CREATE TABLE connections (id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL,
           driver TEXT NOT NULL, color TEXT, read_only INTEGER NOT NULL DEFAULT 0, host TEXT,
           port INTEGER, database TEXT, username TEXT, file_path TEXT,
           options TEXT NOT NULL DEFAULT '{}', created_at INTEGER NOT NULL,
           updated_at INTEGER NOT NULL, tls_mode TEXT NOT NULL DEFAULT 'disable',
           tls_ca_file TEXT, tls_cert_file TEXT, tls_key_file TEXT,
           has_db_password INTEGER NOT NULL DEFAULT 0);
         INSERT INTO connections (id, name, driver, file_path, created_at, updated_at, has_db_password)
           VALUES ('11111111-1111-4111-8111-111111111111', 'old', 'postgres', NULL, 0, 0, 1);
         PRAGMA user_version = 2;",
    )
    .unwrap();
    drop(old);

    let store = Store::open(&path).unwrap();

    assert_eq!(
        store.list_connections().unwrap()[0].saved_secrets,
        vec![SecretKind::DbPassword]
    );
}

fn known(host: &str, fingerprint: &str) -> KnownHost {
    KnownHost {
        host: host.into(),
        port: 22,
        algorithm: "ssh-ed25519".into(),
        fingerprint: fingerprint.into(),
    }
}

#[test]
fn trusted_hosts_are_listed_and_removed() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();

    store
        .trust_host(&known("Bastion.Example.com", "SHA256:a"))
        .unwrap();
    store
        .trust_host(&known("bastion.example.com", "SHA256:a"))
        .unwrap();
    let listed = store.list_known_hosts().unwrap();
    store
        .remove_known_host("BASTION.example.com", 22, "ssh-ed25519")
        .unwrap();

    assert_eq!(listed, vec![known("bastion.example.com", "SHA256:a")]);
    assert!(store.list_known_hosts().unwrap().is_empty());
    assert!(matches!(
        store.remove_known_host("x", 22, "ssh-ed25519"),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn a_different_key_is_never_silently_replaced() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();
    store.trust_host(&known("bastion", "SHA256:a")).unwrap();

    let result = store.trust_host(&known("bastion", "SHA256:b"));

    assert!(matches!(result, Err(StoreError::Invalid(_))));
    assert_eq!(
        store.list_known_hosts().unwrap(),
        vec![known("bastion", "SHA256:a")]
    );
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
    assert!(profiles[0].saved_secrets.is_empty());
}

#[test]
fn always_preview_is_saved_and_updated() {
    let (_dir, path) = temp_path();
    let store = Store::open(&path).unwrap();

    let saved = store
        .save_connection(ConnectionDraft {
            always_preview: true,
            folder: None,
            ..sqlite_draft("prod")
        })
        .unwrap();
    let updated = store
        .save_connection(ConnectionDraft {
            always_preview: false,
            folder: None,
            ..ConnectionDraft::from(saved.clone())
        })
        .unwrap();

    assert!(saved.always_preview);
    assert!(!updated.always_preview);
}
