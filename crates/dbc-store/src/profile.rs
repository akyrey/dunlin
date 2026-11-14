//! Connection profile types. Secrets are never part of a profile; only which ones are saved.

use std::collections::BTreeMap;

use dbc_core::driver::{DriverKind, TlsMode};
use serde::{Deserialize, Serialize};

use crate::secrets::SecretKind;

/// A saved connection as shown to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub driver: DriverKind,
    /// `#rrggbb`.
    pub color: Option<String>,
    pub read_only: bool,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub user: Option<String>,
    pub file_path: Option<String>,
    pub options: BTreeMap<String, String>,
    pub tls: TlsSettings,
    /// SSH tunnel, when the database is reached through one.
    pub ssh: Option<SshSettings>,
    /// Secrets stored in the keychain for this connection. The values are never returned.
    pub saved_secrets: Vec<SecretKind>,
    /// Show the SQL preview before every apply of edits, even after it was skipped once.
    pub always_preview: bool,
    /// The sidebar folder the connection is listed in; `None` for the top level.
    pub folder: Option<String>,
}

impl ConnectionProfile {
    pub fn has_secret(&self, kind: SecretKind) -> bool {
        self.saved_secrets.contains(&kind)
    }
}

/// TLS settings as saved; files are referenced by path, never copied.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TlsSettings {
    pub mode: TlsMode,
    pub ca_file: Option<String>,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum SshAuthMethod {
    Agent,
    KeyFile,
    Password,
}

/// One SSH server: the bastion, or the jump host in front of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SshHostSettings {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: SshAuthMethod,
    /// Private key path for `key_file` authentication; read at connect time, never copied.
    pub key_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SshSettings {
    pub server: SshHostSettings,
    /// Reached first; `server` is reached through it.
    pub jump: Option<SshHostSettings>,
}

/// A profile to save: without `id` it is created, with `id` it replaces that profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDraft {
    pub id: Option<String>,
    pub name: String,
    pub driver: DriverKind,
    pub color: Option<String>,
    #[serde(default)]
    pub read_only: bool,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub user: Option<String>,
    pub file_path: Option<String>,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    #[serde(default)]
    pub tls: TlsSettings,
    #[serde(default)]
    pub ssh: Option<SshSettings>,
    #[serde(default)]
    pub always_preview: bool,
    #[serde(default)]
    pub folder: Option<String>,
}

impl From<ConnectionProfile> for ConnectionDraft {
    fn from(profile: ConnectionProfile) -> Self {
        Self {
            id: Some(profile.id),
            name: profile.name,
            driver: profile.driver,
            color: profile.color,
            read_only: profile.read_only,
            host: profile.host,
            port: profile.port,
            database: profile.database,
            user: profile.user,
            file_path: profile.file_path,
            options: profile.options,
            tls: profile.tls,
            ssh: profile.ssh,
            always_preview: profile.always_preview,
            folder: profile.folder,
        }
    }
}
