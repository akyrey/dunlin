//! TLS modes against a TLS-only Redis with a certificate for `localhost` signed by a test CA.
//! `cargo test -p dbc-driver-redis --test tls -- --ignored`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::PathBuf;

use dbc_core::DbErrorCode;
use dbc_core::driver::{ConnectionConfig, Driver, Endpoint, TlsConfig, TlsMode};
use dbc_driver_redis::RedisDriver;
use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use testcontainers_modules::redis::Redis;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{CopyTargetOptions, ImageExt};

/// The copied files are root-only; give them to `redis`, then serve TLS only, on the usual port.
const START: &str = "install -o redis -m 644 /tls/ca.pem /tmp/ca.pem \\
    && install -o redis -m 644 /tls/server.crt /tmp/server.crt \\
    && install -o redis -m 600 /tls/server.key /tmp/server.key \\
    && exec docker-entrypoint.sh redis-server --port 0 --tls-port 6379 \\
       --tls-cert-file /tmp/server.crt --tls-key-file /tmp/server.key \\
       --tls-ca-cert-file /tmp/ca.pem --tls-auth-clients no";

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
        user: None,
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

/// Connects and pings; the error code when either fails.
async fn reach(config: &ConnectionConfig) -> Result<(), DbErrorCode> {
    let connection = RedisDriver.connect(config).await.map_err(|e| e.code)?;
    let pinged = connection.ping().await.map_err(|e| e.code);
    connection.close().await.unwrap();
    pinged
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn tls_modes_verify_as_documented() {
    let pki = pki();
    let container = Redis::default()
        .with_tag("7.4")
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
    let port = container.get_host_port_ipv4(6379).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, &pki.ca_pem).unwrap();
    let ca = || Some(ca.clone());

    let full = reach(&config(port, "localhost", TlsMode::VerifyFull, ca())).await;
    let wrong_host = reach(&config(
        port,
        "cache.wrong.example",
        TlsMode::VerifyFull,
        ca(),
    ))
    .await;
    let ca_only = reach(&config(
        port,
        "cache.wrong.example",
        TlsMode::VerifyCa,
        ca(),
    ))
    .await;
    let untrusted = reach(&config(port, "localhost", TlsMode::VerifyFull, None)).await;
    let untrusted_ca_only = reach(&config(port, "localhost", TlsMode::VerifyCa, None)).await;
    let require = reach(&config(port, "localhost", TlsMode::Require, None)).await;
    let disable = reach(&config(port, "localhost", TlsMode::Disable, None)).await;

    assert_eq!(full, Ok(()), "verify-full with the CA");
    assert_eq!(
        wrong_host,
        Err(DbErrorCode::Tls),
        "verify-full checks the name"
    );
    assert_eq!(
        ca_only,
        Ok(()),
        "verify-ca trusts the chain whatever the name"
    );
    assert_eq!(
        untrusted,
        Err(DbErrorCode::Tls),
        "verify-full refuses an unknown CA"
    );
    assert_eq!(
        untrusted_ca_only,
        Err(DbErrorCode::Tls),
        "verify-ca refuses an unknown CA"
    );
    assert_eq!(
        require,
        Ok(()),
        "require encrypts without checking the certificate"
    );
    assert!(
        disable.is_err(),
        "a TLS-only server refuses plain text: {disable:?}"
    );
}
