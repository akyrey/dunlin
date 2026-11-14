use std::time::Instant;

use dbc_core::DEFAULT_DISPLAY_LIMIT;
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_core::sql::{self, StatementClass, byte_to_utf16};

use crate::error::AppError;
use crate::ipc::{
    QueryExecuteArgs, QueryOutcome, QueryResult, SqlSplitArgs, StatementRange, TabArgs,
};
use crate::state::AppState;

/// Upper bound for `rowLimit`; results are returned in one response.
pub const MAX_ROW_LIMIT: usize = 100_000;

pub async fn sql_split(
    state: &AppState,
    args: SqlSplitArgs,
) -> Result<Vec<StatementRange>, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let rules = session.dialect.splitter_rules();
    let ranges = sql::split(&args.text, rules)
        .into_iter()
        .map(|r| StatementRange {
            start: byte_to_utf16(&args.text, r.start),
            end: byte_to_utf16(&args.text, r.end),
            danger: args
                .text
                .get(r.start..r.end)
                .and_then(|statement| sql::classify(statement, rules).danger)
                .map(|danger| danger.reason().to_owned()),
        })
        .collect();
    Ok(ranges)
}

/// Runs one statement on the tab's connection after the read-only and danger checks.
pub async fn query_execute(
    state: &AppState,
    args: QueryExecuteArgs,
) -> Result<QueryResult, AppError> {
    if args.sql.trim().is_empty() {
        return Err(AppError::invalid("There is no statement to run"));
    }
    if !(1..=MAX_ROW_LIMIT).contains(&args.row_limit) {
        return Err(AppError::invalid(format!(
            "The row limit must be between 1 and {MAX_ROW_LIMIT}"
        )));
    }
    let session = state.sessions.get(&args.session_id).await?;
    let rules = session.dialect.splitter_rules();
    // Safeguards classify one statement; MySQL would run a whole script sent at once.
    if sql::split(&args.sql, rules).len() > 1 {
        return Err(AppError::invalid("Run one statement at a time"));
    }
    let classification = sql::classify(&args.sql, rules);
    if session.read_only && !classification.class.allowed_on_read_only() {
        return Err(AppError::read_only());
    }
    if let Some(danger) = classification.danger
        && args.confirmed != Some(true)
    {
        return Err(AppError::confirmation_required(danger));
    }
    let started = Instant::now();
    let started_at = super::history::now_millis();
    let options = ExecuteOptions {
        row_limit: args.row_limit,
    };
    let executed = session.execute(&args.tab_id, &args.sql, &options).await;
    // DDL can change what the schema browser shows, even when it fails part-way (MySQL).
    if classification.class == StatementClass::Ddl {
        session.invalidate_schema();
    }
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    super::history::record(
        state,
        super::history::Ran {
            connection_id: &session.connection_id,
            statement: &args.sql,
            started_at,
            duration_ms: elapsed_ms,
            outcome: executed.as_ref(),
        },
    )
    .await;
    let outcome = match executed {
        Ok(outcome) => outcome,
        Err(_) if session.tunnel_lost() => return Err(tunnel_lost(state, &session.id).await),
        Err(error) => return Err(AppError::from_db(error, &args.sql)),
    };
    Ok(QueryResult {
        outcome: to_ipc(outcome),
        elapsed_ms,
        statement_class: classification.class,
    })
}

/// The session's SSH tunnel dropped: the session is closed so it can be reopened cleanly once
/// the server is back (task 3.8).
pub(super) async fn tunnel_lost(state: &AppState, session_id: &str) -> AppError {
    // The session is unusable either way; a failure to close it changes nothing for the user.
    let _ = state.sessions.close(session_id).await;
    AppError::from(dbc_tunnel::TunnelError::Lost)
}

/// Cancels the statement running in a tab, if any.
pub async fn query_cancel(state: &AppState, args: TabArgs) -> Result<(), AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    session.cancel(&args.tab_id).await.map_err(AppError::from)
}

fn to_ipc(outcome: StatementOutcome) -> QueryOutcome {
    match outcome {
        StatementOutcome::Rows(result) => QueryOutcome::Rows {
            columns: result.columns,
            rows: result
                .rows
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|v| v.into_wire(DEFAULT_DISPLAY_LIMIT))
                        .collect()
                })
                .collect(),
            truncated: result.truncated,
        },
        StatementOutcome::Affected { rows } => QueryOutcome::Affected {
            rows: rows.to_string(),
        },
    }
}
