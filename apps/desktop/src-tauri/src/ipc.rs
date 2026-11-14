//! Argument and return types of the Tauri commands. TypeScript is generated from these.

use dbc_core::WireValue;
use dbc_core::driver::{Capabilities, DriverKind};
use dbc_core::result::ColumnMeta;
use dbc_core::schema::{NamespaceRef, TableRef};
use dbc_core::sql::StatementClass;
use dbc_store::{ConnectionDraft, SecretKind};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSaveArgs {
    pub profile: ConnectionDraft,
    #[serde(default)]
    #[ts(optional)]
    pub secrets: Option<ConnectionSecrets>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestArgs {
    pub profile: ConnectionDraft,
    #[serde(default)]
    #[ts(optional)]
    pub secrets: Option<ConnectionSecrets>,
}

/// Secrets only ever travel from the frontend to the backend (docs/IPC.md#secrets-over-ipc).
/// For each field: absent keeps the stored secret, `null` deletes it, a string replaces it.
#[derive(Clone, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSecrets {
    #[serde(default, deserialize_with = "present")]
    #[ts(optional, type = "string | null")]
    pub db_password: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[ts(optional, type = "string | null")]
    pub ssh_password: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[ts(optional, type = "string | null")]
    pub ssh_key_passphrase: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[ts(optional, type = "string | null")]
    pub jump_password: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[ts(optional, type = "string | null")]
    pub jump_key_passphrase: Option<Option<String>>,
}

impl ConnectionSecrets {
    /// Each field with the secret kind it is stored under.
    pub fn changes(&self) -> [(SecretKind, &Option<Option<String>>); 5] {
        [
            (SecretKind::DbPassword, &self.db_password),
            (SecretKind::SshPassword, &self.ssh_password),
            (SecretKind::SshKeyPassphrase, &self.ssh_key_passphrase),
            (SecretKind::JumpPassword, &self.jump_password),
            (SecretKind::JumpKeyPassphrase, &self.jump_key_passphrase),
        ]
    }
}

impl std::fmt::Debug for ConnectionSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("ConnectionSecrets");
        for (kind, change) in self.changes() {
            let state = match change {
                None => "unchanged",
                Some(None) => "cleared",
                Some(Some(_)) => "[redacted]",
            };
            debug.field(kind.as_str(), &state);
        }
        debug.finish()
    }
}

/// Trusts an SSH host key after the user confirmed its fingerprint.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshTrustHostArgs {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshKnownHostRemoveArgs {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
}

/// Distinguishes a field set to `null` (`Some(None)`) from an absent one (`None`, via default).
fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionIdArgs {
    pub connection_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub server_version: String,
    #[ts(type = "number")]
    pub latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: String,
    pub connection_id: String,
    pub driver: DriverKind,
    pub capabilities: Capabilities,
    pub read_only: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionIdArgs {
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SchemaObjectsArgs {
    pub session_id: String,
    pub namespace: NamespaceRef,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SchemaTableArgs {
    pub session_id: String,
    pub table: TableRef,
}

/// Text for a table's context actions, quoted for the session's dialect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TableSnippets {
    pub qualified_name: String,
    /// `SELECT <every column> FROM <qualified name>;`
    pub select: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SqlSplitArgs {
    pub session_id: String,
    pub text: String,
}

/// A statement's range in UTF-16 code units, as JavaScript and CodeMirror count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StatementRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct QueryExecuteArgs {
    pub session_id: String,
    pub tab_id: String,
    pub sql: String,
    pub row_limit: usize,
    #[serde(default)]
    #[ts(optional)]
    pub confirmed: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TabArgs {
    pub session_id: String,
    pub tab_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub outcome: QueryOutcome,
    #[ts(type = "number")]
    pub elapsed_ms: u64,
    pub statement_class: StatementClass,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QueryOutcome {
    Rows {
        columns: Vec<ColumnMeta>,
        rows: Vec<Vec<WireValue>>,
        truncated: bool,
    },
    /// The count as a decimal string, like every database number (DECISIONS D-003).
    Affected { rows: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets(json: &str) -> ConnectionSecrets {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn secrets_distinguish_absent_null_and_value() {
        assert!(secrets("{}").db_password.is_none());
        assert_eq!(secrets(r#"{"dbPassword": null}"#).db_password, Some(None));
        assert_eq!(
            secrets(r#"{"dbPassword": "p"}"#).db_password,
            Some(Some("p".into()))
        );
    }

    #[test]
    fn secrets_debug_output_is_redacted() {
        let debug = format!("{:?}", secrets(r#"{"dbPassword": "hunter2"}"#));

        assert!(!debug.contains("hunter2"), "{debug}");
    }
}
