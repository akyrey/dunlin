//! TLS modes against a server with a certificate for `localhost` signed by a test CA.
//! `cargo test -p dbc-driver-postgres --test tls -- --ignored`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::path::PathBuf;

use dbc_core::DbErrorCode;
use dbc_core::Value;
use dbc_core::driver::{ConnectionConfig, Driver, Endpoint, TlsConfig, TlsMode};
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_driver_postgres::PostgresDriver;
use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::CopyTargetOptions;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

use common::Server;

struct Pki {
    ca_pem: String,
    cert_pem: String,
    key_pem: String,
}

fn pki() -> Pki {
    let ca_key = KeyPair::generate().unwrap();
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca = ca_params.self_signed(&ca_key).unwrap();
    let issuer = Issuer::new(ca_params, ca_key);
    let server_key = KeyPair::generate().unwrap();
    let server = CertificateParams::new(vec!["localhost".to_owned()])
        .unwrap()
        .signed_by(&server_key, &issuer)
        .unwrap();
    Pki {
        ca_pem: ca.pem(),
        cert_pem: server.pem(),
        key_pem: server_key.serialize_pem(),
    }
}

async fn tls_server(pki: &Pki) -> Server {
    // The copied directory is root-only and the key must belong to `postgres`. The entrypoint
    // runs a non-`postgres` command as root, so install the files, then start normally.
    const START: &str = "install -o postgres -m 644 /tls/server.crt /tmp/server.crt \\
        && install -o postgres -m 600 /tls/server.key /tmp/server.key \\
        && exec docker-entrypoint.sh postgres -c ssl=on \\
           -c ssl_cert_file=/tmp/server.crt -c ssl_key_file=/tmp/server.key";
    let container = Postgres::default()
        .with_tag(common::TAG)
        .with_copy_to(
            CopyTargetOptions::new("/tls/server.crt"),
            pki.cert_pem.clone().into_bytes(),
        )
        .with_copy_to(
            CopyTargetOptions::new("/tls/server.key"),
            pki.key_pem.clone().into_bytes(),
        )
        .with_cmd(["sh", "-c", START])
        .start()
        .await
        .unwrap();
    Server::from(container).await
}

fn config(
    server: &Server,
    server_name: &str,
    mode: TlsMode,
    ca_file: Option<PathBuf>,
) -> ConnectionConfig {
    let mut config = server.config("postgres", false);
    config.endpoint = Endpoint::Tcp {
        dial_host: "127.0.0.1".into(),
        dial_port: server.port,
        server_name: server_name.into(),
    };
    config.tls = TlsConfig {
        mode,
        ca_file,
        ..TlsConfig::default()
    };
    config
}

async fn uses_tls(config: &ConnectionConfig) -> Result<bool, DbErrorCode> {
    let connection = PostgresDriver.connect(config).await.map_err(|e| e.code)?;
    let outcome = connection
        .execute(
            "SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()",
            &ExecuteOptions::default(),
        )
        .await
        .map_err(|e| e.code)?;
    let StatementOutcome::Rows(result) = outcome else {
        panic!("expected rows")
    };
    Ok(result.rows[0][0] == Value::Bool(true))
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn tls_modes_verify_as_documented() {
    let pki = pki();
    let server = tls_server(&pki).await;
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, &pki.ca_pem).unwrap();

    let full = uses_tls(&config(
        &server,
        "localhost",
        TlsMode::VerifyFull,
        Some(ca.clone()),
    ))
    .await;
    let wrong_host = uses_tls(&config(
        &server,
        "db.wrong.example",
        TlsMode::VerifyFull,
        Some(ca.clone()),
    ))
    .await;
    let ca_only = uses_tls(&config(
        &server,
        "db.wrong.example",
        TlsMode::VerifyCa,
        Some(ca.clone()),
    ))
    .await;
    let untrusted = uses_tls(&config(&server, "localhost", TlsMode::VerifyFull, None)).await;
    let require = uses_tls(&config(&server, "localhost", TlsMode::Require, None)).await;
    let prefer = uses_tls(&config(&server, "localhost", TlsMode::Prefer, None)).await;
    let disable = uses_tls(&config(&server, "localhost", TlsMode::Disable, None)).await;

    assert_eq!(full, Ok(true), "verify-full with the right name and CA");
    assert_eq!(
        wrong_host,
        Err(DbErrorCode::Tls),
        "verify-full refuses a certificate for another host"
    );
    assert_eq!(ca_only, Ok(true), "verify-ca ignores the name");
    assert_eq!(
        untrusted,
        Err(DbErrorCode::Tls),
        "verify-full refuses an untrusted CA"
    );
    assert_eq!(require, Ok(true), "require encrypts without verifying");
    assert_eq!(prefer, Ok(true), "prefer uses TLS when offered");
    assert_eq!(disable, Ok(false), "disable never uses TLS");
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn require_fails_against_a_server_without_tls_and_prefer_falls_back() {
    let server = Server::start().await;

    let require = uses_tls(&config(&server, "localhost", TlsMode::Require, None)).await;
    let prefer = uses_tls(&config(&server, "localhost", TlsMode::Prefer, None)).await;

    assert!(require.is_err(), "{require:?}");
    assert_eq!(prefer, Ok(false));
}
