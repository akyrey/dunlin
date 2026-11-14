//! Secrets reach the keychain and the driver, and nowhere else (SECURITY.md#secrets).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use dbc_core::dialect::Dialect;
use dbc_core::driver::{
    CancelHandle, Connection, ConnectionConfig, Driver, DriverInfo, DriverKind, EndpointKind,
};
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_core::{DbError, DbErrorCode};
use dbc_driver_postgres::PostgresDialect;
use dbc_store::{ConnectionDraft, MemorySecretStore, SecretKind, SecretStore, TlsSettings};
use dunlin_lib::commands;
use dunlin_lib::ipc::{
    ConnectionIdArgs, ConnectionSaveArgs, ConnectionSecrets, ConnectionTestArgs, QueryExecuteArgs,
};
use dunlin_lib::registry::DriverRegistry;
use dunlin_lib::state::AppState;
use secrecy::ExposeSecret;
use tempfile::TempDir;

const PASSWORD: &str = "S3cr3t-Unique-Value-7f1c";

/// A PostgreSQL-shaped driver that records passwords and can lose its connection on demand.
#[derive(Default)]
struct FakeDriver {
    passwords: Mutex<Vec<Option<String>>>,
    connects: AtomicUsize,
    lose_next: Arc<AtomicBool>,
}

struct FakeConnection {
    lose_next: Arc<AtomicBool>,
}

struct NoCancel;

#[async_trait]
impl CancelHandle for NoCancel {
    async fn cancel(&self) -> Result<(), DbError> {
        Ok(())
    }
}

#[async_trait]
impl Driver for FakeDriver {
    fn info(&self) -> DriverInfo {
        DriverInfo {
            kind: DriverKind::Postgres,
            label: "Fake".into(),
            endpoint: EndpointKind::Tcp,
            default_port: Some(5432),
            tls_modes: Vec::new(),
            capabilities: Default::default(),
            options: Vec::new(),
        }
    }

    fn dialect(&self) -> Arc<dyn Dialect> {
        Arc::new(PostgresDialect)
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<Arc<dyn Connection>, DbError> {
        let password = config
            .password
            .as_ref()
            .map(|p| p.expose_secret().to_owned());
        self.passwords.lock().unwrap().push(password);
        self.connects.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(FakeConnection {
            lose_next: Arc::clone(&self.lose_next),
        }))
    }
}

#[async_trait]
impl Connection for FakeConnection {
    async fn ping(&self) -> Result<(), DbError> {
        Ok(())
    }
    async fn server_version(&self) -> Result<String, DbError> {
        Ok("Fake 1".into())
    }
    async fn execute(&self, _: &str, _: &ExecuteOptions) -> Result<StatementOutcome, DbError> {
        if self.lose_next.swap(false, Ordering::SeqCst) {
            return Err(DbError::new(
                DbErrorCode::ConnectionLost,
                "server closed the connection",
            ));
        }
        Ok(StatementOutcome::Affected { rows: 0 })
    }
    fn cancel_handle(&self) -> Arc<dyn CancelHandle> {
        Arc::new(NoCancel)
    }
    async fn close(&self) -> Result<(), DbError> {
        Ok(())
    }
}

struct Fixture {
    dir: TempDir,
    state: AppState,
    secrets: Arc<MemorySecretStore>,
    driver: Arc<FakeDriver>,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecretStore::default());
        let driver = Arc::new(FakeDriver::default());
        let registry = DriverRegistry::with(vec![driver.clone()]);
        let state =
            AppState::with_registry(&dir.path().join("store.db"), secrets.clone(), registry)
                .unwrap();
        Self {
            dir,
            state,
            secrets,
            driver,
        }
    }

    async fn save(
        &self,
        draft: ConnectionDraft,
        password: Option<Option<&str>>,
    ) -> dbc_store::ConnectionProfile {
        let secrets = password.map(|p| ConnectionSecrets {
            db_password: Some(p.map(str::to_owned)),
            ..Default::default()
        });
        commands::connection_save(
            &self.state,
            ConnectionSaveArgs {
                profile: draft,
                secrets,
            },
        )
        .await
        .unwrap()
    }

    fn stored(&self, id: &str) -> Option<String> {
        self.secrets
            .get(id, SecretKind::DbPassword)
            .unwrap()
            .map(|s| s.expose_secret().to_owned())
    }

    /// Everything on disk in the store directory, including the WAL.
    fn store_bytes(&self) -> Vec<u8> {
        std::fs::read_dir(self.dir.path())
            .unwrap()
            .flat_map(|entry| std::fs::read(entry.unwrap().path()).unwrap())
            .collect()
    }
}

fn draft() -> ConnectionDraft {
    ConnectionDraft {
        id: None,
        name: "prod".into(),
        driver: DriverKind::Postgres,
        color: None,
        read_only: false,
        host: Some("db.example.com".into()),
        port: None,
        database: Some("app".into()),
        user: Some("app".into()),
        file_path: None,
        options: BTreeMap::new(),
        tls: TlsSettings::default(),
        ssh: None,
        always_preview: false,
        folder: None,
    }
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

#[tokio::test]
async fn a_saved_password_is_in_the_keychain_and_nowhere_in_the_store_file() {
    let fixture = Fixture::new();

    let profile = fixture.save(draft(), Some(Some(PASSWORD))).await;

    assert_eq!(fixture.stored(&profile.id).as_deref(), Some(PASSWORD));
    assert!(profile.has_secret(SecretKind::DbPassword));
    assert!(
        !contains(&fixture.store_bytes(), PASSWORD),
        "password found in the store file"
    );
}

#[tokio::test]
async fn no_command_response_contains_the_secret() {
    let fixture = Fixture::new();
    let saved = fixture.save(draft(), Some(Some(PASSWORD))).await;
    let id = ConnectionIdArgs {
        connection_id: saved.id.clone(),
    };

    let responses = [
        serde_json::to_string(&saved).unwrap(),
        serde_json::to_string(&commands::connection_list(&fixture.state).await.unwrap()).unwrap(),
        serde_json::to_string(&commands::session_open(&fixture.state, id).await.unwrap()).unwrap(),
        serde_json::to_string(&commands::driver_list(&fixture.state)).unwrap(),
    ];

    for response in responses {
        assert!(!response.contains(PASSWORD), "{response}");
    }
}

#[tokio::test]
async fn omitting_secrets_keeps_the_stored_password_and_null_deletes_it() {
    let fixture = Fixture::new();
    let created = fixture.save(draft(), Some(Some(PASSWORD))).await;

    let renamed = fixture
        .save(
            ConnectionDraft {
                name: "renamed".into(),
                ..ConnectionDraft::from(created.clone())
            },
            None,
        )
        .await;
    let kept = fixture.stored(&created.id);
    let cleared = fixture
        .save(ConnectionDraft::from(renamed.clone()), Some(None))
        .await;

    assert!(renamed.has_secret(SecretKind::DbPassword));
    assert_eq!(kept.as_deref(), Some(PASSWORD));
    assert!(!cleared.has_secret(SecretKind::DbPassword));
    assert_eq!(fixture.stored(&created.id), None);
}

#[tokio::test]
async fn opening_a_session_passes_the_stored_password_to_the_driver() {
    let fixture = Fixture::new();
    let saved = fixture.save(draft(), Some(Some(PASSWORD))).await;

    commands::session_open(
        &fixture.state,
        ConnectionIdArgs {
            connection_id: saved.id,
        },
    )
    .await
    .unwrap();

    assert_eq!(
        *fixture.driver.passwords.lock().unwrap(),
        vec![Some(PASSWORD.to_owned())]
    );
}

#[tokio::test]
async fn testing_uses_the_given_password_or_else_the_stored_one() {
    let fixture = Fixture::new();
    let saved = fixture.save(draft(), Some(Some(PASSWORD))).await;
    let typed = ConnectionSecrets {
        db_password: Some(Some("typed".into())),
        ..Default::default()
    };

    commands::connection_test(
        &fixture.state,
        ConnectionTestArgs {
            profile: draft(),
            secrets: Some(typed),
        },
    )
    .await
    .unwrap();
    commands::connection_test(
        &fixture.state,
        ConnectionTestArgs {
            profile: ConnectionDraft::from(saved),
            secrets: None,
        },
    )
    .await
    .unwrap();
    commands::connection_test(
        &fixture.state,
        ConnectionTestArgs {
            profile: draft(),
            secrets: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(
        *fixture.driver.passwords.lock().unwrap(),
        vec![Some("typed".to_owned()), Some(PASSWORD.to_owned()), None]
    );
}

#[tokio::test]
async fn a_duplicate_has_its_own_copy_of_the_secrets() {
    let fixture = Fixture::new();
    let saved = fixture.save(draft(), Some(Some(PASSWORD))).await;
    let other = fixture
        .save(
            ConnectionDraft {
                name: "other".into(),
                ..draft()
            },
            None,
        )
        .await;

    let copy = commands::connection_duplicate(
        &fixture.state,
        ConnectionIdArgs {
            connection_id: saved.id.clone(),
        },
    )
    .await
    .unwrap();
    commands::connection_delete(
        &fixture.state,
        ConnectionIdArgs {
            connection_id: saved.id.clone(),
        },
    )
    .await
    .unwrap();
    let listed = commands::connection_list(&fixture.state).await.unwrap();

    assert_eq!(copy.saved_secrets, vec![SecretKind::DbPassword]);
    assert_eq!(fixture.stored(&copy.id).as_deref(), Some(PASSWORD));
    let ids: Vec<&str> = listed.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec![copy.id.as_str(), other.id.as_str()]);
}

#[tokio::test]
async fn connections_are_reordered_as_a_whole() {
    let fixture = Fixture::new();
    let first = fixture.save(draft(), None).await;
    let second = fixture
        .save(
            ConnectionDraft {
                name: "second".into(),
                ..draft()
            },
            None,
        )
        .await;

    let listed = commands::connection_reorder(
        &fixture.state,
        dunlin_lib::ipc::ConnectionReorderArgs {
            order: vec![second.id.clone(), first.id.clone()],
        },
    )
    .await
    .unwrap();
    let partial = commands::connection_reorder(
        &fixture.state,
        dunlin_lib::ipc::ConnectionReorderArgs {
            order: vec![first.id.clone()],
        },
    )
    .await
    .unwrap_err();

    assert_eq!(listed[0].id, second.id);
    assert_eq!(partial.code, "invalid_argument");
}

#[tokio::test]
async fn deleting_a_connection_deletes_its_keychain_entries() {
    let fixture = Fixture::new();
    let saved = fixture.save(draft(), Some(Some(PASSWORD))).await;

    commands::connection_delete(
        &fixture.state,
        ConnectionIdArgs {
            connection_id: saved.id.clone(),
        },
    )
    .await
    .unwrap();

    assert!(fixture.secrets.is_empty());
}

#[tokio::test]
async fn a_lost_connection_is_reported_and_the_next_run_reconnects() {
    let fixture = Fixture::new();
    let saved = fixture.save(draft(), None).await;
    let session = commands::session_open(
        &fixture.state,
        ConnectionIdArgs {
            connection_id: saved.id,
        },
    )
    .await
    .unwrap();
    let run = || QueryExecuteArgs {
        session_id: session.session_id.clone(),
        tab_id: "t".into(),
        sql: "SELECT 1".into(),
        row_limit: 10,
        confirmed: None,
    };
    commands::query_execute(&fixture.state, run())
        .await
        .unwrap();
    fixture.driver.lose_next.store(true, Ordering::SeqCst);

    let lost = commands::query_execute(&fixture.state, run())
        .await
        .err()
        .map(|e| e.code);
    let retried = commands::query_execute(&fixture.state, run()).await;

    assert_eq!(lost.as_deref(), Some("connection_lost"));
    assert!(retried.is_ok());
    // Metadata connection, first tab connection, and the reconnect after the loss.
    assert_eq!(fixture.driver.connects.load(Ordering::SeqCst), 3);
}
