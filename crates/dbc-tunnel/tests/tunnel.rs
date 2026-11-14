//! Spike 3.1 and tasks 3.3–3.5: `cargo test -p dbc-tunnel --test tunnel -- --ignored`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::sync::Arc;
use std::time::Duration;

use dbc_tunnel::{FingerprintList, SshAuth, TrustedKey, Tunnel, TunnelError};
use secrecy::SecretString;
use tokio::net::TcpStream;

use common::{Keys, PASSPHRASE, SshServer, banner, config, key_auth, password_auth, trusted_store};

#[tokio::test]
#[ignore = "needs Docker"]
async fn an_unknown_host_key_is_reported_with_its_fingerprint_and_then_trusted() {
    let keys = Keys::generate();
    let server = SshServer::start(&keys, None).await;
    let config = config(server.host(key_auth(&keys)));

    let first = Tunnel::open(&config, Arc::new(FingerprintList::default()))
        .await
        .err();
    let store = trusted_store(&config).await;
    let tunnel = Tunnel::open(&config, store).await.unwrap();

    let Some(TunnelError::HostKeyUnknown { fingerprint, .. }) = first else {
        panic!("{first:?}")
    };
    assert!(fingerprint.starts_with("SHA256:"));
    assert!(banner(&tunnel).await.starts_with("SSH-2.0-OpenSSH"));
    tunnel.close().await.unwrap();
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn a_changed_host_key_is_refused() {
    let keys = Keys::generate();
    let server = SshServer::start(&keys, None).await;
    let config = config(server.host(key_auth(&keys)));
    let real = trusted_store(&config).await;
    let forged = FingerprintList {
        entries: real
            .entries
            .iter()
            .map(|e| TrustedKey {
                fingerprint: "SHA256:not-the-real-key".into(),
                ..e.clone()
            })
            .collect(),
    };

    let result = Tunnel::open(&config, Arc::new(forged)).await.err();

    assert!(
        matches!(result, Some(TunnelError::HostKeyChanged { .. })),
        "{result:?}"
    );
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn password_and_encrypted_key_authentication() {
    let keys = Keys::generate();
    let server = SshServer::start(&keys, None).await;
    let store = trusted_store(&config(server.host(key_auth(&keys)))).await;
    let encrypted = |passphrase: Option<&str>| SshAuth::KeyFile {
        path: keys.encrypted.clone(),
        passphrase: passphrase.map(|p| SecretString::from(p.to_owned())),
    };
    let open = |auth: SshAuth| {
        let store = store.clone();
        let config = config(server.host(auth));
        async move { Tunnel::open(&config, store).await }
    };

    let password = open(password_auth()).await;
    let wrong_password = open(SshAuth::Password(SecretString::from("nope")))
        .await
        .err();
    let with_passphrase = open(encrypted(Some(PASSPHRASE))).await;
    let without_passphrase = open(encrypted(None)).await.err();

    assert!(banner(&password.unwrap()).await.starts_with("SSH-2.0"));
    assert!(
        matches!(wrong_password, Some(TunnelError::Auth { .. })),
        "{wrong_password:?}"
    );
    assert!(
        banner(&with_passphrase.unwrap())
            .await
            .starts_with("SSH-2.0")
    );
    let Some(TunnelError::Auth { reason, .. }) = without_passphrase else {
        panic!("{without_passphrase:?}")
    };
    assert!(reason.contains("passphrase"), "{reason}");
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn several_connections_share_one_tunnel() {
    let keys = Keys::generate();
    let server = SshServer::start(&keys, None).await;
    let config = config(server.host(key_auth(&keys)));
    let tunnel = Tunnel::open(&config, trusted_store(&config).await)
        .await
        .unwrap();

    let (a, b, c) = tokio::join!(banner(&tunnel), banner(&tunnel), banner(&tunnel));

    assert!([a, b, c].iter().all(|b| b.starts_with("SSH-2.0")));
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn no_listener_remains_after_close() {
    let keys = Keys::generate();
    let server = SshServer::start(&keys, None).await;
    let config = config(server.host(key_auth(&keys)));
    let tunnel = Tunnel::open(&config, trusted_store(&config).await)
        .await
        .unwrap();
    let address = tunnel.local_addr();

    tunnel.close().await.unwrap();

    assert!(
        TcpStream::connect(address).await.is_err(),
        "{address} still accepts connections"
    );
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn an_unreachable_server_is_a_connect_error() {
    let keys = Keys::generate();
    let mut config = config(dbc_tunnel::SshHost {
        host: "127.0.0.1".into(),
        port: 1,
        user: "x".into(),
        auth: key_auth(&keys),
    });
    config.keepalive = Duration::from_secs(1);

    let result = Tunnel::open(&config, Arc::new(FingerprintList::default()))
        .await
        .err();

    assert!(
        matches!(result, Some(TunnelError::Connect { .. })),
        "{result:?}"
    );
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn losing_the_ssh_server_is_detected_and_the_listener_closes() {
    let keys = Keys::generate();
    let server = SshServer::start(&keys, None).await;
    let config = config(server.host(key_auth(&keys)));
    let tunnel = Tunnel::open(&config, trusted_store(&config).await)
        .await
        .unwrap();

    server.container.stop().await.unwrap();
    let detected = tokio::time::timeout(Duration::from_secs(15), async {
        while !tunnel.is_lost() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    // The accept loop ends once the loss is seen.
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(detected.is_ok(), "loss not detected");
    assert!(TcpStream::connect(tunnel.local_addr()).await.is_err());
    tunnel.close().await.unwrap();
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn a_target_the_server_cannot_reach_is_refused_when_opening() {
    let keys = Keys::generate();
    let server = SshServer::start(&keys, None).await;
    let mut config = config(server.host(key_auth(&keys)));
    let store = trusted_store(&config).await;
    config.target_host = "no-such-host.invalid".into();

    let result = Tunnel::open(&config, store).await.err();

    assert!(
        matches!(result, Some(TunnelError::ForwardRefused { .. })),
        "{result:?}"
    );
}
