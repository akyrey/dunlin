//! Query history: every statement run from the editor, per connection, capped.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{Store, StoreError};

/// Entries kept per connection unless the caller asks otherwise (ARCHITECTURE.md#store).
pub const DEFAULT_HISTORY_CAP: usize = 10_000;
/// Upper bound for one `list_history` page.
pub const MAX_HISTORY_PAGE: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum HistoryOutcome {
    Rows,
    Affected,
    Error,
}

impl HistoryOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Rows => "rows",
            Self::Affected => "affected",
            Self::Error => "error",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "rows" => Self::Rows,
            "affected" => Self::Affected,
            _ => Self::Error,
        }
    }
}

/// What the app records after running a statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHistoryEntry {
    pub connection_id: String,
    pub statement: String,
    /// Milliseconds since the Unix epoch.
    pub started_at: i64,
    pub duration_ms: u64,
    pub outcome: HistoryOutcome,
    /// Rows read or affected.
    pub row_count: Option<u64>,
    pub truncated: bool,
    /// The error message, for `Error`.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    pub connection_id: String,
    pub statement: String,
    pub started_at: i64,
    pub duration_ms: u64,
    pub outcome: HistoryOutcome,
    pub row_count: Option<u64>,
    pub truncated: bool,
    pub error: Option<String>,
}

/// A page of history, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryQuery<'a> {
    /// Case-insensitive substring of the statement.
    pub search: Option<&'a str>,
    pub limit: usize,
    /// Only entries older than this id (the last id of the previous page).
    pub before: Option<i64>,
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

impl Store {
    /// Records one statement and drops the connection's oldest entries beyond `cap`.
    pub fn record_history(&self, entry: &NewHistoryEntry, cap: usize) -> Result<i64, StoreError> {
        self.connection.execute(
            "INSERT INTO query_history
                (connection_id, statement, started_at, duration_ms, outcome, row_count, truncated, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                entry.connection_id,
                entry.statement,
                entry.started_at,
                to_i64(entry.duration_ms),
                entry.outcome.as_str(),
                entry.row_count.map(to_i64),
                entry.truncated,
                entry.error,
            ],
        )?;
        let id = self.connection.last_insert_rowid();
        let cutoff: Option<i64> = self
            .connection
            .query_row(
                "SELECT id FROM query_history WHERE connection_id = ?1
                 ORDER BY id DESC LIMIT 1 OFFSET ?2",
                params![entry.connection_id, to_i64(cap as u64)],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(cutoff) = cutoff {
            self.connection.execute(
                "DELETE FROM query_history WHERE connection_id = ?1 AND id <= ?2",
                params![entry.connection_id, cutoff],
            )?;
        }
        Ok(id)
    }

    pub fn list_history(
        &self,
        connection_id: &str,
        query: &HistoryQuery<'_>,
    ) -> Result<Vec<HistoryEntry>, StoreError> {
        let limit = query.limit.clamp(1, MAX_HISTORY_PAGE);
        let mut statement = self.connection.prepare(
            "SELECT id, connection_id, statement, started_at, duration_ms, outcome, row_count,
                    truncated, error
             FROM query_history
             WHERE connection_id = ?1
               AND (?2 IS NULL OR instr(lower(statement), lower(?2)) > 0)
               AND (?3 IS NULL OR id < ?3)
             ORDER BY id DESC LIMIT ?4",
        )?;
        let search = query.search.filter(|s| !s.trim().is_empty());
        let rows = statement.query_map(
            params![connection_id, search, query.before, to_i64(limit as u64)],
            |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    connection_id: row.get(1)?,
                    statement: row.get(2)?,
                    started_at: row.get(3)?,
                    duration_ms: u64::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
                    outcome: HistoryOutcome::parse(&row.get::<_, String>(5)?),
                    row_count: row
                        .get::<_, Option<i64>>(6)?
                        .map(|n| u64::try_from(n).unwrap_or(0)),
                    truncated: row.get(7)?,
                    error: row.get(8)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn clear_history(&self, connection_id: &str) -> Result<(), StoreError> {
        self.connection.execute(
            "DELETE FROM query_history WHERE connection_id = ?1",
            [connection_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::collections::BTreeMap;

    use dbc_core::driver::DriverKind;

    use super::*;
    use crate::{ConnectionDraft, TlsSettings};

    fn store_with_connection() -> (tempfile::TempDir, Store, String) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("store.db")).unwrap();
        let profile = store
            .save_connection(ConnectionDraft {
                id: None,
                name: "local".into(),
                driver: DriverKind::Sqlite,
                color: None,
                read_only: false,
                host: None,
                port: None,
                database: None,
                user: None,
                file_path: Some("/tmp/x.db".into()),
                options: BTreeMap::new(),
                tls: TlsSettings::default(),
                ssh: None,
            })
            .unwrap();
        (dir, store, profile.id)
    }

    fn entry(connection_id: &str, statement: &str) -> NewHistoryEntry {
        NewHistoryEntry {
            connection_id: connection_id.into(),
            statement: statement.into(),
            started_at: 1_700_000_000_000,
            duration_ms: 12,
            outcome: HistoryOutcome::Rows,
            row_count: Some(3),
            truncated: false,
            error: None,
        }
    }

    fn page(limit: usize) -> HistoryQuery<'static> {
        HistoryQuery {
            limit,
            ..HistoryQuery::default()
        }
    }

    #[test]
    fn records_and_lists_newest_first_with_outcome() {
        let (_dir, store, id) = store_with_connection();
        store.record_history(&entry(&id, "SELECT 1"), 10).unwrap();
        let failed = NewHistoryEntry {
            outcome: HistoryOutcome::Error,
            row_count: None,
            error: Some("syntax error".into()),
            ..entry(&id, "SELECT * FRM t")
        };
        store.record_history(&failed, 10).unwrap();

        let listed = store.list_history(&id, &page(10)).unwrap();

        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].statement, "SELECT * FRM t");
        assert_eq!(listed[0].outcome, HistoryOutcome::Error);
        assert_eq!(listed[0].error.as_deref(), Some("syntax error"));
        assert_eq!(listed[1].row_count, Some(3));
        assert_eq!(listed[1].duration_ms, 12);
    }

    #[test]
    fn keeps_only_the_newest_entries_per_connection() {
        let (_dir, store, id) = store_with_connection();
        for i in 0..5 {
            store
                .record_history(&entry(&id, &format!("SELECT {i}")), 3)
                .unwrap();
        }

        let statements: Vec<String> = store
            .list_history(&id, &page(10))
            .unwrap()
            .into_iter()
            .map(|e| e.statement)
            .collect();

        assert_eq!(statements, vec!["SELECT 4", "SELECT 3", "SELECT 2"]);
    }

    #[test]
    fn searches_case_insensitively_and_pages_with_before() {
        let (_dir, store, id) = store_with_connection();
        for statement in [
            "select * from users",
            "SELECT 1",
            "delete from USERS where id = 1",
            "SELECT '%_'",
        ] {
            store.record_history(&entry(&id, statement), 10).unwrap();
        }

        let users = store
            .list_history(
                &id,
                &HistoryQuery {
                    search: Some("Users"),
                    limit: 10,
                    before: None,
                },
            )
            .unwrap();
        let literal = store
            .list_history(
                &id,
                &HistoryQuery {
                    search: Some("%_"),
                    limit: 10,
                    before: None,
                },
            )
            .unwrap();
        let first = store.list_history(&id, &page(2)).unwrap();
        let second = store
            .list_history(
                &id,
                &HistoryQuery {
                    before: first.last().map(|e| e.id),
                    ..page(2)
                },
            )
            .unwrap();

        assert_eq!(users.len(), 2);
        assert_eq!(literal.len(), 1);
        assert_eq!(first[0].statement, "SELECT '%_'");
        assert_eq!(
            second
                .iter()
                .map(|e| e.statement.as_str())
                .collect::<Vec<_>>(),
            vec!["SELECT 1", "select * from users"]
        );
    }

    #[test]
    fn clearing_and_deleting_the_connection_remove_its_history() {
        let (_dir, store, id) = store_with_connection();
        store.record_history(&entry(&id, "SELECT 1"), 10).unwrap();
        store.clear_history(&id).unwrap();
        assert!(store.list_history(&id, &page(10)).unwrap().is_empty());

        store.record_history(&entry(&id, "SELECT 2"), 10).unwrap();
        store.delete_connection(&id).unwrap();
        let left: i64 = store
            .connection
            .query_row("SELECT count(*) FROM query_history", [], |row| row.get(0))
            .unwrap();

        assert_eq!(left, 0);
    }
}
