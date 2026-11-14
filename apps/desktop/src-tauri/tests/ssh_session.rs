//! Phase 3 acceptance at the command level: host key trust, failure stages, tunnel loss and
//! no leftover listener. `cargo test -p dunlin --test ssh_session -- --ignored`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use dbc_core::driver::DriverKind;
use dbc_store::{
    ConnectionDraft, ConnectionProfile, MemorySecretStore, SshAuthMethod, SshHostSettings,
    SshSettings, TlsSettings,
};
use dbc_tunnel::test_support::{Keys, SshServer};
use dunlin_lib::commands;
use dunlin_lib::error::AppError;
use dunlin_lib::ipc::{
    ConnectionIdArgs, ConnectionSaveArgs, ConnectionSecrets, ConnectionTestArgs, QueryExecuteArgs,
    SessionIdArgs, SessionInfo, SshTrustHostArgs,
};
use dunlin_lib::state::AppState;
use serde_json::Value as Json;
use tempfile::TempDir;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio::net::TcpStream;

struct World {
    _dir: TempDir,
    state: AppState,
    keys: Keys,
    ssh: SshServer,
    _database: ContainerAsync<Postgres>,
    database_name: String,
}

impl World {
    async fn start(tag: &str) -> Self {
        let id = std::process::id();
        let network = format!("dunlin-app-{tag}-{id}");
        let database_name = format!("dunlin-app-{tag}-pg-{id}");
        let keys = Keys::generate();
        let database = Postgres::default()
            .with_tag("18-alpine")
            .with_network(network.clone())
            .with_container_name(database_name.clone())
            .start()
            .await
            .unwrap();
        let ssh = SshServer::start(
            &keys,
            Some((&network, &format!("dunlin-app-{tag}-ssh-{id}"))),
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(
            &dir.path().join("store.db"),
            Arc::new(MemorySecretStore::default()),
        )
        .unwrap();
        Self {
            _dir: dir,
            state,
            keys,
            ssh,
            _database: database,
            database_name,
        }
    }

    fn draft(&self, ssh_port: u16) -> ConnectionDraft {
        ConnectionDraft {
            id: None,
            name: "behind bastion".into(),
            driver: DriverKind::Postgres,
            color: None,
            read_only: false,
            host: Some(self.database_name.clone()),
            port: Some(5432),
            database: Some("postgres".into()),
            user: Some("postgres".into()),
            file_path: None,
            options: BTreeMap::new(),
            tls: TlsSettings::default(),
            ssh: Some(SshSettings {
                server: SshHostSettings {
                    host: "127.0.0.1".into(),
                    port: ssh_port,
                    user: dbc_tunnel::test_support::USER.into(),
                    auth: SshAuthMethod::KeyFile,
                    key_file: Some(self.keys.plain.to_string_lossy().into_owned()),
                },
                jump: None,
            }),
        }
    }

    async fn save(&self, draft: ConnectionDraft) -> ConnectionProfile {
        let secrets = ConnectionSecrets {
            db_password: Some(Some("postgres".into())),
            ..Default::default()
        };
        commands::connection_save(
            &self.state,
            ConnectionSaveArgs {
                profile: draft,
                secrets: Some(secrets),
            },
        )
        .await
        .unwrap()
    }

    async fn open(&self, profile: &ConnectionProfile) -> Result<SessionInfo, AppError> {
        commands::session_open(
            &self.state,
            ConnectionIdArgs {
                connection_id: profile.id.clone(),
            },
        )
        .await
    }

    async fn trust(&self, error: &AppError) {
        commands::ssh_trust_host(&self.state, trust_args(error))
            .await
            .unwrap();
    }

    async fn run(&self, session: &SessionInfo, sql: &str) -> Result<(), AppError> {
        let args = QueryExecuteArgs {
            session_id: session.session_id.clone(),
            tab_id: "t".into(),
            sql: sql.into(),
            row_limit: 10,
            confirmed: None,
        };
        commands::query_execute(&self.state, args).await.map(|_| ())
    }
}

fn detail<'a>(error: &'a AppError, field: &str) -> &'a Json {
    error
        .detail
        .as_ref()
        .and_then(|d| d.get(field))
        .unwrap_or(&Json::Null)
}

fn trust_args(error: &AppError) -> SshTrustHostArgs {
    SshTrustHostArgs {
        host: detail(error, "host").as_str().unwrap().into(),
        port: u16::try_from(detail(error, "port").as_u64().unwrap()).unwrap(),
        algorithm: detail(error, "algorithm").as_str().unwrap().into(),
        fingerprint: detail(error, "fingerprint").as_str().unwrap().into(),
    }
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn unknown_keys_prompt_once_and_closing_leaves_no_listener() {
    let world = World::start("trust").await;
    let profile = world.save(world.draft(world.ssh.port)).await;

    let unknown = world.open(&profile).await.err().unwrap();
    let after_declining = commands::ssh_known_hosts_list(&world.state).await.unwrap();
    world.trust(&unknown).await;
    let session = world.open(&profile).await.unwrap();
    let address = world
        .state
        .sessions
        .get(&session.session_id)
        .await
        .unwrap()
        .tunnel_addr()
        .unwrap();
    world.run(&session, "SELECT 1").await.unwrap();
    commands::session_close(
        &world.state,
        SessionIdArgs {
            session_id: session.session_id.clone(),
        },
    )
    .await
    .unwrap();
    let second = world.open(&profile).await;

    assert_eq!(unknown.code, "ssh_host_key_unknown");
    assert!(
        detail(&unknown, "fingerprint")
            .as_str()
            .unwrap()
            .starts_with("SHA256:")
    );
    assert!(after_declining.is_empty(), "declining must store nothing");
    assert!(
        TcpStream::connect(address).await.is_err(),
        "listener left bound at {address}"
    );
    assert!(
        second.is_ok(),
        "a trusted key must not prompt again: {second:?}"
    );
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn a_changed_key_is_refused_and_cannot_be_trusted_over() {
    let world = World::start("changed").await;
    let profile = world.save(world.draft(world.ssh.port)).await;
    let unknown = world.open(&profile).await.err().unwrap();
    let mut forged = trust_args(&unknown);
    forged.fingerprint = "SHA256:someone-else".into();
    commands::ssh_trust_host(&world.state, forged)
        .await
        .unwrap();

    let changed = world.open(&profile).await.err().unwrap();
    let overwrite = commands::ssh_trust_host(&world.state, trust_args(&changed))
        .await
        .err()
        .unwrap();

    assert_eq!(changed.code, "ssh_host_key_changed");
    assert_eq!(detail(&changed, "stage"), "host_key");
    assert_eq!(overwrite.code, "invalid_argument");
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn test_connection_reports_the_failing_stage() {
    let world = World::start("stages").await;
    let draft = world.draft(world.ssh.port);
    let test = |draft: ConnectionDraft, db_password: &str| {
        let secrets = ConnectionSecrets {
            db_password: Some(Some(db_password.into())),
            ..Default::default()
        };
        commands::connection_test(
            &world.state,
            ConnectionTestArgs {
                profile: draft,
                secrets: Some(secrets),
            },
        )
    };
    let stage = |error: AppError| detail(&error, "stage").as_str().unwrap().to_owned();

    let host_key = stage(test(draft.clone(), "postgres").await.err().unwrap());
    let unknown = world
        .open(&world.save(draft.clone()).await)
        .await
        .err()
        .unwrap();
    world.trust(&unknown).await;
    let db_auth = stage(test(draft.clone(), "wrong").await.err().unwrap());
    let mut bad_key = draft.clone();
    if let Some(ssh) = &mut bad_key.ssh {
        ssh.server.key_file = Some(
            world
                .keys
                .dir
                .path()
                .join("missing")
                .to_string_lossy()
                .into_owned(),
        );
    }
    let ssh_auth = stage(test(bad_key, "postgres").await.err().unwrap());
    let mut no_server = draft.clone();
    if let Some(ssh) = &mut no_server.ssh {
        ssh.server.port = 1;
    }
    let ssh_connect = stage(test(no_server, "postgres").await.err().unwrap());
    let mut no_database = draft.clone();
    no_database.host = Some("no-such-database-host".into());
    let forward = stage(test(no_database, "postgres").await.err().unwrap());
    let ok = test(draft, "postgres").await;

    assert_eq!(host_key, "host_key");
    assert_eq!(db_auth, "db_auth");
    assert_eq!(ssh_auth, "ssh_auth");
    assert_eq!(ssh_connect, "ssh_connect");
    assert_eq!(forward, "forward");
    assert!(ok.unwrap().server_version.starts_with("PostgreSQL"));
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn losing_the_ssh_server_reports_tunnel_lost_and_reopening_works_when_it_is_back() {
    let world = World::start("lost").await;
    let profile = world.save(world.draft(world.ssh.port)).await;
    let unknown = world.open(&profile).await.err().unwrap();
    world.trust(&unknown).await;
    let session = world.open(&profile).await.unwrap();
    world.run(&session, "SELECT 1").await.unwrap();

    world.ssh.container.stop().await.unwrap();
    let mut lost = None;
    for _ in 0..40 {
        match world.run(&session, "SELECT 1").await {
            Err(error) if error.code == "tunnel_lost" => {
                lost = Some(error);
                break;
            }
            _ => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
    world.ssh.container.start().await.unwrap();
    let port = world.ssh.container.get_host_port_ipv4(22).await.unwrap();
    let moved = world
        .save(ConnectionDraft {
            id: Some(profile.id.clone()),
            ..world.draft(port)
        })
        .await;
    // sshd needs a few seconds after a restart, and the container may get a new port, which
    // is a different host:port to trust.
    let mut reopened = Err(AppError::new("not_tried", ""));
    for _ in 0..60 {
        reopened = match world.open(&moved).await {
            Err(error) if error.code == "ssh_host_key_unknown" => {
                world.trust(&error).await;
                world.open(&moved).await
            }
            other => other,
        };
        if reopened.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    assert!(lost.is_some(), "tunnel_lost was never reported");
    assert!(
        world.state.sessions.get(&session.session_id).await.is_err(),
        "the lost session is closed"
    );
    let reopened = reopened.unwrap();
    assert!(world.run(&reopened, "SELECT 1").await.is_ok());
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn the_schema_browser_reports_tunnel_lost_too() {
    let world = World::start("lost-schema").await;
    let profile = world.save(world.draft(world.ssh.port)).await;
    let unknown = world.open(&profile).await.err().unwrap();
    world.trust(&unknown).await;
    let session = world.open(&profile).await.unwrap();
    let id = || dunlin_lib::ipc::SessionIdArgs {
        session_id: session.session_id.clone(),
    };
    assert!(
        commands::schema_namespaces(&world.state, id())
            .await
            .is_ok()
    );

    world.ssh.container.stop().await.unwrap();
    let mut code = String::new();
    for _ in 0..40 {
        // Each attempt skips the cache, so it has to reach the server.
        let _ = commands::schema_refresh(&world.state, id()).await;
        match commands::schema_namespaces(&world.state, id()).await {
            Err(error) if error.code == "tunnel_lost" || error.code == "not_found" => {
                code = error.code;
                break;
            }
            _ => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }

    assert_eq!(code, "tunnel_lost");
    assert!(world.state.sessions.get(&session.session_id).await.is_err());
}
