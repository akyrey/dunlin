//! Agent authentication against a private `ssh-agent`. `cargo test -p dbc-tunnel --test agent -- --ignored`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use dbc_tunnel::{SshAuth, Tunnel, TunnelError};

use common::{Keys, SshServer, banner, config, key_auth, trusted_store};

/// An `ssh-agent` on its own socket, killed when dropped.
struct Agent {
    child: Child,
    socket: std::path::PathBuf,
}

impl Agent {
    fn start(dir: &std::path::Path) -> Self {
        let socket = dir.join("agent.sock");
        let child = Command::new("ssh-agent")
            .arg("-D")
            .arg("-a")
            .arg(&socket)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        for _ in 0..50 {
            if socket.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Self { child, socket }
    }

    fn add(&self, key: &std::path::Path) {
        let status = Command::new("ssh-add")
            .arg("-q")
            .arg(key)
            .env("SSH_AUTH_SOCK", &self.socket)
            .status()
            .unwrap();
        assert!(status.success());
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[tokio::test]
#[ignore = "needs Docker and ssh-agent"]
async fn the_agent_authenticates_with_a_key_it_holds() {
    let keys = Keys::generate();
    let server = SshServer::start(&keys, None).await;
    let store = trusted_store(&config(server.host(key_auth(&keys)))).await;
    let agent = Agent::start(keys.dir.path());
    let auth = SshAuth::Agent {
        socket: agent.socket.clone(),
    };

    let empty = Tunnel::open(&config(server.host(auth.clone())), store.clone())
        .await
        .err();
    agent.add(&keys.plain);
    let tunnel = Tunnel::open(&config(server.host(auth)), store)
        .await
        .unwrap();

    let Some(TunnelError::Auth { reason, .. }) = empty else {
        panic!("{empty:?}")
    };
    assert!(reason.contains("no keys"), "{reason}");
    assert!(banner(&tunnel).await.starts_with("SSH-2.0"));
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn a_missing_agent_is_an_auth_error() {
    let keys = Keys::generate();
    let server = SshServer::start(&keys, None).await;
    let store = trusted_store(&config(server.host(key_auth(&keys)))).await;
    let auth = SshAuth::Agent {
        socket: keys.dir.path().join("no-agent.sock"),
    };

    let result = Tunnel::open(&config(server.host(auth)), store).await.err();

    let Some(TunnelError::Auth { reason, .. }) = result else {
        panic!("{result:?}")
    };
    assert!(reason.contains("SSH agent"), "{reason}");
}
