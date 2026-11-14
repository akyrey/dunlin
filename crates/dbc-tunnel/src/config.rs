//! Tunnel configuration (docs/phases/phase-3-ssh-tunnels.md#design).

use std::path::PathBuf;
use std::time::Duration;

use secrecy::SecretString;

/// Default interval between SSH keepalives.
pub const DEFAULT_KEEPALIVE: Duration = Duration::from_secs(15);
/// Missed keepalives after which the SSH connection counts as lost.
pub const KEEPALIVE_MAX_MISSED: usize = 3;

#[derive(Debug, Clone)]
pub enum SshAuth {
    /// Keys offered by the SSH agent listening on `socket` (the app passes `SSH_AUTH_SOCK`).
    Agent {
        socket: PathBuf,
    },
    /// A private key file, read at connect time and never copied.
    KeyFile {
        path: PathBuf,
        passphrase: Option<SecretString>,
    },
    Password(SecretString),
}

/// One SSH hop. The jump host, if any, is the first hop and carries the second.
#[derive(Debug, Clone)]
pub struct SshHost {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: SshAuth,
}

impl SshAuth {
    /// Agent authentication through the agent named by `SSH_AUTH_SOCK`, if one is set.
    pub fn agent_from_env() -> Option<Self> {
        std::env::var_os("SSH_AUTH_SOCK").map(|socket| Self::Agent {
            socket: socket.into(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TunnelConfig {
    pub ssh: SshHost,
    /// Reached first; `ssh` is then reached through it.
    pub jump: Option<SshHost>,
    /// The database host as seen from the SSH server.
    pub target_host: String,
    pub target_port: u16,
    pub keepalive: Duration,
}
