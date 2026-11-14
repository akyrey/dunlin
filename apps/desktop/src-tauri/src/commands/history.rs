//! Query history (tasks 5.11, 5.12): every statement the editor runs is recorded with its
//! duration and outcome. SQL text is stored only here; it is never logged (SECURITY.md).

use std::time::{SystemTime, UNIX_EPOCH};

use dbc_core::DbError;
use dbc_core::result::StatementOutcome;
use dbc_store::{DEFAULT_HISTORY_CAP, HistoryEntry, HistoryOutcome, HistoryQuery, NewHistoryEntry};

use crate::error::AppError;
use crate::ipc::{ConnectionIdArgs, HistoryListArgs};
use crate::state::AppState;

/// Milliseconds since the Unix epoch.
pub(crate) fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// What one run produced, for the history row.
pub(crate) struct Ran<'a> {
    pub connection_id: &'a str,
    pub statement: &'a str,
    pub started_at: i64,
    pub duration_ms: u64,
    pub outcome: Result<&'a StatementOutcome, &'a DbError>,
}

fn entry(ran: &Ran<'_>) -> NewHistoryEntry {
    let (outcome, row_count, truncated, error) = match ran.outcome {
        Ok(StatementOutcome::Rows(result)) => (
            HistoryOutcome::Rows,
            Some(result.rows.len() as u64),
            result.truncated,
            None,
        ),
        Ok(StatementOutcome::Affected { rows }) => {
            (HistoryOutcome::Affected, Some(*rows), false, None)
        }
        Err(error) => (
            HistoryOutcome::Error,
            None,
            false,
            Some(error.message.clone()),
        ),
    };
    NewHistoryEntry {
        connection_id: ran.connection_id.to_owned(),
        statement: ran.statement.to_owned(),
        started_at: ran.started_at,
        duration_ms: ran.duration_ms,
        outcome,
        row_count,
        truncated,
        error,
    }
}

/// Records a run. Failing to record never fails the statement, which already ran; it is
/// logged without the SQL.
pub(crate) async fn record(state: &AppState, ran: Ran<'_>) {
    let entry = entry(&ran);
    let result = state
        .with_store(move |store| {
            store
                .record_history(&entry, DEFAULT_HISTORY_CAP)
                .map_err(AppError::from)
        })
        .await;
    if let Err(error) = result {
        tracing::warn!(code = %error.code, "query history was not recorded");
    }
}

pub async fn history_list(
    state: &AppState,
    args: HistoryListArgs,
) -> Result<Vec<HistoryEntry>, AppError> {
    state
        .with_store(move |store| {
            let query = HistoryQuery {
                search: args.search.as_deref(),
                limit: args.limit,
                before: args.before,
            };
            store
                .list_history(&args.connection_id, &query)
                .map_err(AppError::from)
        })
        .await
}

pub async fn history_clear(state: &AppState, args: ConnectionIdArgs) -> Result<(), AppError> {
    state
        .with_store(move |store| {
            store
                .clear_history(&args.connection_id)
                .map_err(AppError::from)
        })
        .await
}
