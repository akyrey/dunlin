//! Table and server statistics (Phase 10, D-043). Every field is optional: each server knows
//! different things, and the frontend shows what is there.

use serde::Serialize;

/// Sizes, row estimate, index usage and maintenance of one table.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TableStats {
    /// Table, indexes and everything else stored for it.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub total_bytes: Option<u64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub table_bytes: Option<u64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub index_bytes: Option<u64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub estimated_rows: Option<u64>,
    /// Every index, including those that back keys.
    pub indexes: Vec<IndexStats>,
    /// Rows deleted or updated away but not yet reclaimed (PostgreSQL).
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub dead_rows: Option<u64>,
    /// Last vacuum and analyze, manual or automatic, as the server prints them.
    pub last_vacuum: Option<String>,
    pub last_analyze: Option<String>,
    /// When the table last changed (MySQL `update_time`).
    pub updated_at: Option<String>,
    /// Since when index scans are counted, when the server says.
    pub counted_since: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub name: String,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub bytes: Option<u64>,
    /// Times the index was used; `Some(0)` is an unused index, `None` unknown.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub scans: Option<u64>,
}

/// How many statements the query statistics view lists.
pub const QUERY_STATS_LIMIT: usize = 50;

/// One normalised statement the server has been recording.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct QueryStat {
    /// As the server normalised it (literals replaced by `$1` or `?`).
    pub statement: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub calls: u64,
    pub total_ms: f64,
    pub mean_ms: f64,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub rows: Option<u64>,
}

/// The heaviest statements, or why the server is not recording them.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QueryStats {
    Available {
        statements: Vec<QueryStat>,
    },
    /// `reason` says how to turn recording on.
    Unavailable {
        reason: String,
    },
}

/// Turns a server's signed count into a size or count; negative means unknown
/// (PostgreSQL's `reltuples` is -1 before the first analyze).
pub fn count(value: Option<i64>) -> Option<u64> {
    value.and_then(|v| u64::try_from(v).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_counts_are_unknown() {
        assert_eq!(count(Some(-1)), None);
        assert_eq!(count(Some(0)), Some(0));
        assert_eq!(count(None), None);
    }
}
