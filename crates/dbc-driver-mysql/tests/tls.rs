//! TLS modes against MySQL with a certificate for `localhost` signed by a test CA.
//! `cargo test -p dbc-driver-mysql --test tls -- --ignored`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::PathBuf;

use dbc_core::driver::{ConnectionConfig, Driver, Endpoint, TlsConfig, TlsMode};
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_core::{DbErrorCode, Value};
use dbc_driver_mysql::MysqlDriver;
use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{CopyTargetOptions, ImageExt};

/// The copied directory is root-only; give the files to `mysql`, then start normally.
const START: &str = "install -o mysql -m 644 /tls/ca.pem /tmp/ca.pem \\
    && install -o mysql -m 644 /tls/server.crt /tmp/server.crt \\
    && install -o mysql -m 600 /tls/server.key /tmp/server.key \\
    && exec docker-entrypoint.sh mysqld --ssl-ca=/tmp/ca.pem \\
       --ssl-cert=/tmp/server.crt --ssl-key=/tmp/server.key";

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
    let key = KeyPair::generate().unwrap();
    let cert = CertificateParams::new(vec!["localhost".to_owned()])
        .unwrap()
        .signed_by(&key, &issuer)
        .unwrap();
    Pki {
        ca_pem: ca.pem(),
        cert_pem: cert.pem(),
        key_pem: key.serialize_pem(),
    }
}

fn config(
    port: u16,
    server_name: &str,
    mode: TlsMode,
    ca_file: Option<PathBuf>,
) -> ConnectionConfig {
    ConnectionConfig {
        endpoint: Endpoint::Tcp {
            dial_host: "127.0.0.1".into(),
            dial_port: port,
            server_name: server_name.into(),
        },
        database: None,
        user: Some("root".into()),
        password: None,
        tls: TlsConfig {
            mode,
            ca_file,
            ..TlsConfig::default()
        },
        read_only: false,
        options: Default::default(),
    }
}

async fn cipher(config: &ConnectionConfig) -> Result<String, DbErrorCode> {
    let connection = MysqlDriver.connect(config).await.map_err(|e| e.code)?;
    let outcome = connection
        .execute(
            "SHOW SESSION STATUS LIKE 'Ssl_cipher'",
            &ExecuteOptions::default(),
        )
        .await
        .map_err(|e| e.code)?;
    let StatementOutcome::Rows(result) = outcome else {
        panic!("expected rows")
    };
    match &result.rows[0][1] {
        Value::Text(cipher) => Ok(cipher.clone()),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn tls_modes_verify_as_documented() {
    let pki = pki();
    let container = Mysql::default()
        .with_tag("8.4")
        .with_copy_to(
            CopyTargetOptions::new("/tls/ca.pem"),
            pki.ca_pem.clone().into_bytes(),
        )
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
    let port = container.get_host_port_ipv4(3306).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, &pki.ca_pem).unwrap();

    let full = cipher(&config(
        port,
        "localhost",
        TlsMode::VerifyFull,
        Some(ca.clone()),
    ))
    .await;
    let wrong_host = cipher(&config(
        port,
        "db.wrong.example",
        TlsMode::VerifyFull,
        Some(ca.clone()),
    ))
    .await;
    let ca_only = cipher(&config(
        port,
        "db.wrong.example",
        TlsMode::VerifyCa,
        Some(ca.clone()),
    ))
    .await;
    let untrusted = cipher(&config(port, "localhost", TlsMode::VerifyFull, None)).await;
    let require = cipher(&config(port, "localhost", TlsMode::Require, None)).await;
    let disable = cipher(&config(port, "localhost", TlsMode::Disable, None)).await;

    assert!(
        full.as_ref().is_ok_and(|c| !c.is_empty()),
        "verify-full: {full:?}"
    );
    assert_eq!(
        wrong_host,
        Err(DbErrorCode::Tls),
        "verify-full refuses a certificate for another host"
    );
    assert_eq!(
        ca_only,
        Err(DbErrorCode::Tls),
        "verify-ca is refused up front (D-018)"
    );
    assert_eq!(
        untrusted,
        Err(DbErrorCode::Tls),
        "verify-full refuses an untrusted CA"
    );
    assert!(
        require.as_ref().is_ok_and(|c| !c.is_empty()),
        "require: {require:?}"
    );
    assert_eq!(disable, Ok(String::new()), "disable never uses TLS");
}
