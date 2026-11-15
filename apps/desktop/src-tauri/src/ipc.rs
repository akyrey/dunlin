//! Argument and return types of the Tauri commands. TypeScript is generated from these.

use dbc_core::WireValue;
use dbc_core::data::SortKey;
use dbc_core::driver::{Capabilities, DriverKind};
use dbc_core::export::ExportFormat;
use dbc_core::result::ColumnMeta;
use dbc_core::schema::{NamespaceRef, TableRef};
use dbc_core::sql::StatementClass;
use dbc_store::{ConnectionDraft, ConnectionProfile, SavedQueryDraft, SecretKind, Settings};
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

/// A host alias from `~/.ssh/config` with the settings a tunnel needs (deferred feature 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshConfigEntry {
    pub alias: String,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    /// The first hop: another alias, or `[user@]host[:port]`.
    pub proxy_jump: Option<String>,
}

impl From<dbc_tunnel::SshConfigHost> for SshConfigEntry {
    fn from(host: dbc_tunnel::SshConfigHost) -> Self {
        Self {
            alias: host.alias,
            host_name: host.host_name,
            user: host.user,
            port: host.port,
            identity_file: host.identity_file,
            proxy_jump: host.proxy_jump,
        }
    }
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

/// Every connection id, in the new order.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionReorderArgs {
    pub order: Vec<String>,
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
    /// Where unqualified names resolve; autocomplete's default schema.
    pub default_namespace: Option<NamespaceRef>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionIdArgs {
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HistoryListArgs {
    pub connection_id: String,
    #[serde(default)]
    #[ts(optional)]
    pub search: Option<String>,
    pub limit: usize,
    /// The id of the last entry of the previous page.
    #[serde(default)]
    #[ts(optional)]
    pub before: Option<i64>,
}

/// `column = value` on a table page: foreign key navigation and cell lookups.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct KeyArg {
    pub column: String,
    pub value: WireValue,
}

/// Table data commands run on the view's own connection (`viewId`, like an editor tab id), so
/// they can be cancelled with `query_cancel` and never wait for the tree or an editor.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TablePageArgs {
    pub session_id: String,
    pub view_id: String,
    pub table: TableRef,
    #[serde(default)]
    #[ts(optional)]
    pub filter: Option<String>,
    #[serde(default)]
    pub key: Vec<KeyArg>,
    #[serde(default)]
    pub sort: Vec<SortKey>,
    pub limit: usize,
    #[ts(type = "number")]
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TablePage {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<WireValue>>,
    /// Another page follows.
    pub has_more: bool,
    #[ts(type = "number")]
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TableCountArgs {
    pub session_id: String,
    pub view_id: String,
    pub table: TableRef,
    #[serde(default)]
    #[ts(optional)]
    pub filter: Option<String>,
    #[serde(default)]
    pub key: Vec<KeyArg>,
    /// `false`: the driver's estimate, immediately; `true`: `count(*)`.
    pub exact: bool,
}

/// One staged edit; `key` holds the row key's original values.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationArg {
    Insert { values: Vec<KeyArg> },
    Update { key: Vec<KeyArg>, set: Vec<KeyArg> },
    Delete { key: Vec<KeyArg> },
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ChangesArgs {
    pub session_id: String,
    /// The table view's connection: edits apply where the rows were read.
    pub view_id: String,
    pub table: TableRef,
    pub operations: Vec<OperationArg>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    #[ts(type = "number")]
    pub statements: usize,
    pub rows_affected: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSetArgs {
    pub settings: Settings,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuerySaveArgs {
    pub query: SavedQueryDraft,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SavedQueryIdArgs {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportResultArgs {
    /// The session the rows came from (its dialect writes `INSERT` literals).
    pub session_id: String,
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<WireValue>>,
    pub format: ExportFormat,
    /// Absolute path chosen in the save dialog.
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportTextArgs {
    pub session_id: String,
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<WireValue>>,
    pub format: ExportFormat,
}

/// A whole table, streamed on the view's connection `viewId` (cancel with `query_cancel`).
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportTableArgs {
    pub session_id: String,
    pub view_id: String,
    pub table: TableRef,
    #[serde(default)]
    #[ts(optional)]
    pub filter: Option<String>,
    #[serde(default)]
    pub key: Vec<KeyArg>,
    #[serde(default)]
    pub sort: Vec<SortKey>,
    pub format: ExportFormat,
    pub path: String,
}

/// Sent on the progress channel of `export_table` after each batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub rows_written: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportDone {
    pub rows_written: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CellFullValueArgs {
    pub session_id: String,
    pub view_id: String,
    pub table: TableRef,
    /// The row key of the row, as shown.
    pub key: Vec<KeyArg>,
    pub column: String,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StatementRange {
    pub start: usize,
    pub end: usize,
    /// Why running it needs confirmation, if it does; a script lists these before it starts.
    pub danger: Option<String>,
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

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsExportArgs {
    pub path: String,
    /// The connections to write; all of them when absent.
    #[serde(default)]
    #[ts(optional)]
    pub ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsImportArgs {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: Vec<ConnectionProfile>,
    /// `name: reason` for each entry that was not imported.
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct KvScanArgs {
    pub session_id: String,
    /// A `SCAN MATCH` glob; `*` when absent or empty.
    #[serde(default)]
    #[ts(optional)]
    pub pattern: Option<String>,
    /// The `cursor` of the previous page; absent to start.
    #[serde(default)]
    #[ts(optional)]
    pub cursor: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub count: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct KvReadArgs {
    pub session_id: String,
    /// The key's `id` (base64 of its bytes).
    pub key: String,
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct KvEditArgs {
    pub session_id: String,
    /// The key's `id` (base64 of its bytes); a missing key is created by the first write.
    pub key: String,
    pub edit: dbc_core::kv::KeyEdit,
    /// Only return the commands, without running them.
    pub preview: bool,
}

/// A key's content as a grid (D-032).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct KeyContentIpc {
    pub key: dbc_core::kv::KeyName,
    pub key_type: String,
    #[ts(type = "number | null")]
    pub ttl_ms: Option<i64>,
    /// Elements in the whole value (bytes for a string).
    #[ts(type = "number")]
    pub length: u64,
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<WireValue>>,
    /// Fewer rows than `length` are shown.
    pub truncated: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct QueryExplainArgs {
    pub session_id: String,
    pub tab_id: String,
    pub sql: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TransactionModeArgs {
    pub session_id: String,
    pub tab_id: String,
    pub manual: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TransactionFinishArgs {
    pub session_id: String,
    pub tab_id: String,
    /// `true` commits, `false` rolls back.
    pub commit: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub outcome: QueryOutcome,
    #[ts(type = "number")]
    pub elapsed_ms: u64,
    pub statement_class: StatementClass,
    /// The one table a plain `SELECT` read, resolved on the tab's own connection; its rows can
    /// then be edited like the table's (when its row key is among the columns).
    pub source: Option<TableRef>,
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
