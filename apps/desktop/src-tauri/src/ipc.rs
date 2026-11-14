//! Argument and return types of the Tauri commands. TypeScript is generated from these.

use dbc_core::WireValue;
use dbc_core::driver::{Capabilities, DriverKind};
use dbc_core::result::ColumnMeta;
use dbc_core::sql::StatementClass;
use dbc_store::ConnectionDraft;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSaveArgs {
    pub profile: ConnectionDraft,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestArgs {
    pub profile: ConnectionDraft,
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
