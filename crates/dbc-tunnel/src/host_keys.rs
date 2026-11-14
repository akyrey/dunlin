//! Host key verification (SECURITY.md#ssh).

use std::path::PathBuf;

use russh::keys::{HashAlg, PublicKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyStatus {
    Trusted,
    /// No key of this algorithm is known for the host.
    Unknown,
    /// A different key of the same algorithm is known: refuse, never prompt.
    Changed,
}

pub trait HostKeyStore: Send + Sync {
    fn check(&self, host: &str, port: u16, key: &PublicKey) -> HostKeyStatus;
}

/// `SHA256:...`, as `ssh-keygen -l` prints it.
pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

pub fn algorithm(key: &PublicKey) -> String {
    key.algorithm().to_string()
}

/// The user's `~/.ssh/known_hosts`, read-only. Hashed entries and `[host]:port` are understood.
/// Unreadable or missing files simply know no hosts.
pub struct KnownHostsFile {
    path: PathBuf,
}

impl KnownHostsFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// `~/.ssh/known_hosts`, if there is a home directory.
    pub fn user_default() -> Option<Self> {
        std::env::home_dir().map(|home| Self::new(home.join(".ssh").join("known_hosts")))
    }
}

impl HostKeyStore for KnownHostsFile {
    fn check(&self, host: &str, port: u16, key: &PublicKey) -> HostKeyStatus {
        match russh::keys::check_known_hosts_path(host, port, key, &self.path) {
            Ok(true) => HostKeyStatus::Trusted,
            Ok(false) => HostKeyStatus::Unknown,
            Err(russh::keys::Error::KeyChanged { .. }) => HostKeyStatus::Changed,
            // A file we cannot parse vouches for nothing; the app's own table may still.
            Err(_) => HostKeyStatus::Unknown,
        }
    }
}

/// Several stores: any `Changed` wins, then any `Trusted`, else `Unknown`.
pub struct CompositeHostKeyStore {
    stores: Vec<Box<dyn HostKeyStore>>,
}

impl CompositeHostKeyStore {
    pub fn new(stores: Vec<Box<dyn HostKeyStore>>) -> Self {
        Self { stores }
    }
}

impl HostKeyStore for CompositeHostKeyStore {
    fn check(&self, host: &str, port: u16, key: &PublicKey) -> HostKeyStatus {
        let statuses: Vec<HostKeyStatus> = self
            .stores
            .iter()
            .map(|store| store.check(host, port, key))
            .collect();
        if statuses.contains(&HostKeyStatus::Changed) {
            HostKeyStatus::Changed
        } else if statuses.contains(&HostKeyStatus::Trusted) {
            HostKeyStatus::Trusted
        } else {
            HostKeyStatus::Unknown
        }
    }
}

/// Trusted keys held as `(host, port, algorithm, fingerprint)`, e.g. from the app's table.
#[derive(Debug, Clone, Default)]
pub struct FingerprintList {
    pub entries: Vec<TrustedKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedKey {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
}

impl HostKeyStore for FingerprintList {
    fn check(&self, host: &str, port: u16, key: &PublicKey) -> HostKeyStatus {
        let algorithm = algorithm(key);
        let fingerprint = fingerprint(key);
        let same_algorithm: Vec<&TrustedKey> = self
            .entries
            .iter()
            .filter(|e| {
                e.host.eq_ignore_ascii_case(host) && e.port == port && e.algorithm == algorithm
            })
            .collect();
        if same_algorithm.iter().any(|e| e.fingerprint == fingerprint) {
            HostKeyStatus::Trusted
        } else if same_algorithm.is_empty() {
            HostKeyStatus::Unknown
        } else {
            HostKeyStatus::Changed
        }
    }
}

#[cfg(test)]
#[path = "host_keys_tests.rs"]
mod tests;
