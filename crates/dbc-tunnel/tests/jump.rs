//! Task 3.7: a second SSH server reachable only from the first.
//! `cargo test -p dbc-tunnel --test jump -- --ignored`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::sync::Arc;

use dbc_tunnel::{FingerprintList, SshHost, TrustedKey, Tunnel, TunnelError};

use common::{Keys, SshServer, USER, banner, config, key_auth, trusted_store};

#[tokio::test]
#[ignore = "needs Docker"]
async fn a_server_on_a_private_network_is_reached_through_a_jump_host() {
    let keys = Keys::generate();
    let network = format!("dunlin-jump-{}", std::process::id());
    let inner_name = format!("dunlin-inner-{}", std::process::id());
    let jump = SshServer::start(
        &keys,
        Some((&network, &format!("dunlin-jump-{}", std::process::id()))),
    )
    .await;
    let _inner = SshServer::start(&keys, Some((&network, &inner_name))).await;
    let mut store = trusted_store(&config(jump.host(key_auth(&keys))))
        .await
        .as_ref()
        .clone();
    let inner = SshHost {
        host: inner_name.clone(),
        port: 22,
        user: USER.into(),
        auth: key_auth(&keys),
    };
    let mut config = config(inner);
    config.jump = Some(jump.host(key_auth(&keys)));

    // The inner server's key is checked under its own name, and is unknown at first.
    let first = Tunnel::open(&config, Arc::new(store.clone())).await.err();
    let Some(TunnelError::HostKeyUnknown {
        host,
        port,
        algorithm,
        fingerprint,
    }) = first
    else {
        panic!("{first:?}")
    };
    store.entries.push(TrustedKey {
        host: host.clone(),
        port,
        algorithm,
        fingerprint,
    });
    let tunnel = Tunnel::open(&config, Arc::new(store)).await.unwrap();

    assert_eq!(host, inner_name);
    assert!(banner(&tunnel).await.starts_with("SSH-2.0"));
    tunnel.close().await.unwrap();
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn an_unreachable_second_hop_is_a_forward_error() {
    let keys = Keys::generate();
    let jump = SshServer::start(&keys, None).await;
    let store: FingerprintList = trusted_store(&config(jump.host(key_auth(&keys))))
        .await
        .as_ref()
        .clone();
    let mut config = config(SshHost {
        host: "no-such-host.invalid".into(),
        port: 22,
        user: USER.into(),
        auth: key_auth(&keys),
    });
    config.jump = Some(jump.host(key_auth(&keys)));

    let result = Tunnel::open(&config, Arc::new(store)).await.err();

    assert!(
        matches!(result, Some(TunnelError::ForwardRefused { .. })),
        "{result:?}"
    );
}
