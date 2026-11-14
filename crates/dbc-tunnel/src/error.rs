//! Tunnel errors. Messages never contain secrets.

/// Where in opening the tunnel something failed, for "Test connection" (task 3.12).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TunnelError {
    #[error("Cannot reach the SSH server {host}:{port}: {reason}")]
    Connect {
        host: String,
        port: u16,
        reason: String,
    },
    #[error("The SSH host key of {host}:{port} is not trusted yet ({fingerprint})")]
    HostKeyUnknown {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
    },
    #[error("The SSH host key of {host}:{port} has changed ({fingerprint}); refusing to connect")]
    HostKeyChanged {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
    },
    #[error("SSH authentication as {user} on {host} failed: {reason}")]
    Auth {
        host: String,
        user: String,
        reason: String,
    },
    #[error("The SSH server refused to forward to {target}: {reason}")]
    ForwardRefused { target: String, reason: String },
    #[error("The SSH connection was lost")]
    Lost,
    #[error("SSH error: {0}")]
    Internal(String),
}
