//! Phase 3 acceptance: PostgreSQL reached only through an SSH tunnel.
//! `cargo test -p dbc-driver-postgres --test through_tunnel -- --ignored`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use dbc_core::Value;
use dbc_core::conformance::{self, Harness, SyntaxErrorCase, TypeCase};
use dbc_core::driver::{ConnectionConfig, Driver, Endpoint, TlsConfig, TlsMode};
use dbc_core::result::{ExecuteOptions, StatementOutcome, TypeCategory};
use dbc_driver_postgres::PostgresDriver;
use dbc_tunnel::Tunnel;
use dbc_tunnel::test_support::{Keys, SshServer, config as tunnel_config, key_auth, trusted_store};
use secrecy::SecretString;

use common::Server;

/// The database is administered directly (setup only); the driver under test dials the tunnel.
struct TunnelHarness {
    server: Server,
    tunnel: SocketAddr,
}

impl TunnelHarness {
    fn through_tunnel(&self, mut config: ConnectionConfig) -> ConnectionConfig {
        config.endpoint = Endpoint::Tcp {
            dial_host: self.tunnel.ip().to_string(),
            dial_port: self.tunnel.port(),
            server_name: "localhost".into(),
        };
        config
    }
}

#[async_trait]
impl Harness for TunnelHarness {
    fn driver(&self) -> Arc<dyn Driver> {
        Arc::new(PostgresDriver)
    }

    async fn database(&self, read_only: bool) -> ConnectionConfig {
        let name = self.server.fresh_database().await;
        self.through_tunnel(self.server.config(&name, read_only))
    }

    fn unreachable(&self) -> ConnectionConfig {
        let mut config = self.server.config("postgres", false);
        config.endpoint = Endpoint::Tcp {
            dial_host: "127.0.0.1".into(),
            dial_port: 1,
            server_name: "localhost".into(),
        };
        config
    }

    fn bad_credentials(&self) -> Option<ConnectionConfig> {
        let mut config = self.through_tunnel(self.server.config("postgres", false));
        config.password = Some(SecretString::from("wrong"));
        Some(config)
    }

    fn type_cases(&self) -> Vec<TypeCase> {
        vec![
            TypeCase::new(
                "SELECT 123456789012345678901234567890.5::numeric",
                Value::Decimal("123456789012345678901234567890.5".into()),
            )
            .with_category(TypeCategory::Decimal),
            TypeCase::new("SELECT 'héllo'::text", Value::Text("héllo".into())),
            TypeCase::new("SELECT NULL::int8", Value::Null),
        ]
    }

    fn syntax_error(&self) -> SyntaxErrorCase {
        SyntaxErrorCase {
            sql: "SELECT * FRM customers".into(),
            position: Some(9),
        }
    }

    fn slow_statement(&self) -> String {
        "SELECT pg_sleep(30)".into()
    }
}

/// A database and an SSH server on a private network; returns them with a tunnel to the database.
async fn setup(tag: &str) -> (Keys, SshServer, Server, Tunnel) {
    let network = format!("dunlin-{tag}-{}", std::process::id());
    let database_name = format!("dunlin-{tag}-pg-{}", std::process::id());
    let keys = Keys::generate();
    let server = Server::start_on(&network, &database_name).await;
    let ssh = SshServer::start(
        &keys,
        Some((
            &network,
            &format!("dunlin-{tag}-ssh-{}", std::process::id()),
        )),
    )
    .await;
    let tunnel = open_tunnel(&keys, &ssh, &database_name).await;
    (keys, ssh, server, tunnel)
}

async fn open_tunnel(keys: &Keys, ssh: &SshServer, target: &str) -> Tunnel {
    let mut config = tunnel_config(ssh.host(key_auth(keys)));
    config.target_host = target.into();
    config.target_port = 5432;
    Tunnel::open(&config, trusted_store(&config).await)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn the_conformance_suite_passes_through_a_tunnel() {
    let (_keys, _ssh, server, tunnel) = setup("conf").await;
    let harness = TunnelHarness {
        server,
        tunnel: tunnel.local_addr(),
    };

    // Includes cancelling `pg_sleep` (the cancel request also travels through the tunnel).
    conformance::run_all(&harness).await.unwrap();
    tunnel.close().await.unwrap();
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn verify_full_checks_the_real_host_name_through_a_tunnel() {
    let network = format!("dunlin-tls-{}", std::process::id());
    let database_name = format!("dunlin-tls-pg-{}", std::process::id());
    let keys = Keys::generate();
    let pki = common::pki();
    let server = common::tls_server(&pki, Some((&network, &database_name))).await;
    let ssh = SshServer::start(
        &keys,
        Some((&network, &format!("dunlin-tls-ssh-{}", std::process::id()))),
    )
    .await;
    let tunnel = open_tunnel(&keys, &ssh, &database_name).await;
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, &pki.ca_pem).unwrap();
    let harness = TunnelHarness {
        server,
        tunnel: tunnel.local_addr(),
    };
    let config = |server_name: &str| {
        let mut config = harness.through_tunnel(harness.server.config("postgres", false));
        if let Endpoint::Tcp {
            server_name: name, ..
        } = &mut config.endpoint
        {
            *name = server_name.into();
        }
        config.tls = TlsConfig {
            mode: TlsMode::VerifyFull,
            ca_file: Some(ca.clone()),
            ..TlsConfig::default()
        };
        config
    };

    // The tunnel is dialled at 127.0.0.1, but the certificate is for `localhost`, the real name.
    let real = PostgresDriver.connect(&config("localhost")).await;
    let wrong = PostgresDriver
        .connect(&config("db.wrong.example"))
        .await
        .err()
        .map(|e| e.code);

    let connection = real.unwrap();
    let ssl = connection
        .execute(
            "SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()",
            &ExecuteOptions::default(),
        )
        .await
        .unwrap();
    let StatementOutcome::Rows(ssl) = ssl else {
        panic!("expected rows")
    };
    assert_eq!(ssl.rows[0][0], Value::Bool(true));
    assert_eq!(wrong, Some(dbc_core::DbErrorCode::Tls));
}
