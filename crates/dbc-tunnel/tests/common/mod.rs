//! OpenSSH servers in containers, client keys made with `ssh-keygen`. Needs Docker.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use dbc_tunnel::{
    FingerprintList, SshAuth, SshHost, TrustedKey, Tunnel, TunnelConfig, TunnelError,
};
use secrecy::SecretString;
use tempfile::TempDir;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

pub const USER: &str = "tunnel";
pub const PASSWORD: &str = "tunnel-password";
pub const PASSPHRASE: &str = "key-passphrase";

/// Our settings go first: sshd keeps the first value it reads, and Alpine disables forwarding.
const START: &str = r#"set -e
apk add --no-cache openssh-server >/dev/null
ssh-keygen -A >/dev/null
adduser -D tunnel
echo 'tunnel:tunnel-password' | chpasswd
mkdir -p /home/tunnel/.ssh
cp /keys/authorized_keys /home/tunnel/.ssh/authorized_keys
chown -R tunnel:tunnel /home/tunnel/.ssh
chmod 700 /home/tunnel/.ssh
chmod 600 /home/tunnel/.ssh/authorized_keys
printf 'PasswordAuthentication yes\nAllowTcpForwarding yes\nPermitOpen any\nClientAliveInterval 0\n' \
  | cat - /etc/ssh/sshd_config > /tmp/sshd_config
mv /tmp/sshd_config /etc/ssh/sshd_config
exec /usr/sbin/sshd -D -e
"#;

/// Client key pairs: one plain, one encrypted with `PASSPHRASE`.
pub struct Keys {
    pub dir: TempDir,
    pub plain: PathBuf,
    pub encrypted: PathBuf,
}

impl Keys {
    pub fn generate() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("id_plain");
        let encrypted = dir.path().join("id_encrypted");
        keygen(&plain, "");
        keygen(&encrypted, PASSPHRASE);
        Self {
            dir,
            plain,
            encrypted,
        }
    }

    pub fn authorized_keys(&self) -> String {
        [&self.plain, &self.encrypted]
            .iter()
            .map(|key| std::fs::read_to_string(key.with_extension("pub")).unwrap())
            .collect()
    }
}

fn keygen(path: &Path, passphrase: &str) {
    let status = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-C", "", "-N", passphrase, "-f"])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

pub struct SshServer {
    pub container: ContainerAsync<GenericImage>,
    pub port: u16,
}

impl SshServer {
    pub async fn start(keys: &Keys, network: Option<(&str, &str)>) -> Self {
        let mut request = GenericImage::new("alpine", "3.21")
            .with_exposed_port(22.tcp())
            .with_wait_for(WaitFor::message_on_stderr("Server listening on"))
            .with_copy_to("/keys/authorized_keys", keys.authorized_keys().into_bytes())
            .with_cmd(["sh", "-c", START]);
        if let Some((network, name)) = network {
            request = request.with_network(network).with_container_name(name);
        }
        let container = request.start().await.unwrap();
        let port = container.get_host_port_ipv4(22).await.unwrap();
        Self { container, port }
    }

    pub fn host(&self, auth: SshAuth) -> SshHost {
        SshHost {
            host: "127.0.0.1".into(),
            port: self.port,
            user: USER.into(),
            auth,
        }
    }
}

pub fn key_auth(keys: &Keys) -> SshAuth {
    SshAuth::KeyFile {
        path: keys.plain.clone(),
        passphrase: None,
    }
}

pub fn password_auth() -> SshAuth {
    SshAuth::Password(SecretString::from(PASSWORD))
}

/// A tunnel to the SSH server's own sshd (port 22 inside the container).
pub fn config(ssh: SshHost) -> TunnelConfig {
    TunnelConfig {
        ssh,
        jump: None,
        target_host: "127.0.0.1".into(),
        target_port: 22,
        keepalive: Duration::from_secs(1),
    }
}

/// Learns the host key the way the app does: the first attempt reports it as unknown.
pub async fn trusted_store(config: &TunnelConfig) -> Arc<FingerprintList> {
    let mut store = FingerprintList::default();
    loop {
        match Tunnel::open(config, Arc::new(store.clone())).await {
            Ok(tunnel) => {
                tunnel.close().await.unwrap();
                return Arc::new(store);
            }
            Err(TunnelError::HostKeyUnknown {
                host,
                port,
                algorithm,
                fingerprint,
            }) => {
                store.entries.push(TrustedKey {
                    host,
                    port,
                    algorithm,
                    fingerprint,
                });
            }
            Err(other) => panic!("unexpected {other:?}"),
        }
    }
}

/// Reads the SSH banner the target sends, proving bytes flow through the tunnel.
pub async fn banner(tunnel: &Tunnel) -> String {
    let mut socket = TcpStream::connect(tunnel.local_addr()).await.unwrap();
    let mut buffer = vec![0u8; 64];
    let read = tokio::time::timeout(Duration::from_secs(10), socket.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    String::from_utf8_lossy(&buffer[..read]).into_owned()
}
