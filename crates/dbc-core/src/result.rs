//! Statement results.

use serde::{Deserialize, Serialize};

use crate::value::Value;

/// Default number of rows read from an ad-hoc statement.
pub const DEFAULT_ROW_LIMIT: usize = 1000;

/// What the frontend uses to choose alignment, cell editor and cell viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum TypeCategory {
    Bool,
    Integer,
    Float,
    Decimal,
    Text,
    Json,
    Xml,
    Binary,
    Temporal,
    Uuid,
    Array,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    /// As the database calls it: "int8", "varchar(255)", "JSONB". Empty when unknown.
    pub type_name: String,
    pub category: TypeCategory,
    /// Unknown for arbitrary expressions.
    pub nullable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResultSet {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Value>>,
    /// More rows were available than `row_limit`.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatementOutcome {
    Rows(ResultSet),
    Affected { rows: u64 },
}

/// What `Connection::stream_rows` sends: the columns once, then batches of rows.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Columns(Vec<ColumnMeta>),
    Rows(Vec<Vec<Value>>),
}

/// Rows per `StreamEvent::Rows` batch.
pub const STREAM_BATCH: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecuteOptions {
    pub row_limit: usize,
}

impl Default for ExecuteOptions {
    fn default() -> Self {
        Self {
            row_limit: DEFAULT_ROW_LIMIT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_row_limit_is_one_thousand() {
        assert_eq!(ExecuteOptions::default().row_limit, 1000);
    }
}
