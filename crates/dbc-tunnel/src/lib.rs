//! SSH local port forwarding (DECISIONS D-004): a loopback listener whose connections are
//! carried to the database over `direct-tcpip` channels.

mod auth;
mod config;
mod error;
mod handler;
mod host_keys;
mod tunnel;

pub use config::{DEFAULT_KEEPALIVE, KEEPALIVE_MAX_MISSED, SshAuth, SshHost, TunnelConfig};
pub use error::TunnelError;
pub use host_keys::{
    CompositeHostKeyStore, FingerprintList, HostKeyStatus, HostKeyStore, KnownHostsFile,
    TrustedKey, algorithm, fingerprint,
};
pub use tunnel::Tunnel;
