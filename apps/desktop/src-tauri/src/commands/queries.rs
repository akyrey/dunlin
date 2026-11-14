use std::time::Instant;

use dbc_core::DEFAULT_DISPLAY_LIMIT;
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_core::sql::{self, byte_to_utf16};

use crate::error::AppError;
use crate::ipc::{QueryExecuteArgs, QueryOutcome, QueryResult, SqlSplitArgs, StatementRange};
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
    let classification = sql::classify(&args.sql, session.dialect.splitter_rules());
    if session.read_only && !classification.class.allowed_on_read_only() {
        return Err(AppError::read_only());
    }
    if let Some(danger) = classification.danger
        && args.confirmed != Some(true)
    {
        return Err(AppError::confirmation_required(danger));
    }
    let connection = session.tab_connection(&args.tab_id).await?;
    let started = Instant::now();
    let outcome = connection
        .execute(
            &args.sql,
            &ExecuteOptions {
                row_limit: args.row_limit,
            },
        )
        .await
        .map_err(|e| AppError::from_db(e, &args.sql))?;
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok(QueryResult {
        outcome: to_ipc(outcome),
        elapsed_ms,
        statement_class: classification.class,
    })
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
