use std::time::Instant;

use dbc_core::DEFAULT_DISPLAY_LIMIT;
use dbc_core::DbError;
use dbc_core::plan::PlanNode;
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_core::schema::{NamespaceRef, TableRef};
use dbc_core::sql::{self, StatementClass, byte_to_utf16};

use crate::error::AppError;
use crate::ipc::{
    QueryExecuteArgs, QueryExplainArgs, QueryOutcome, QueryResult, SqlSplitArgs, StatementRange,
    TabArgs, TransactionFinishArgs, TransactionModeArgs,
};
use crate::session::{Session, TransactionState};
use crate::state::AppState;

/// Upper bound for `rowLimit`; results are returned in one response.
pub const MAX_ROW_LIMIT: usize = 100_000;

pub async fn sql_split(
    state: &AppState,
    args: SqlSplitArgs,
) -> Result<Vec<StatementRange>, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let ranges = session
        .dialect
        .split(&args.text)
        .into_iter()
        .map(|r| StatementRange {
            start: byte_to_utf16(&args.text, r.start),
            end: byte_to_utf16(&args.text, r.end),
            danger: args
                .text
                .get(r.start..r.end)
                .and_then(|statement| session.dialect.classify(statement).danger)
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
    // Safeguards classify one statement; MySQL would run a whole script sent at once.
    if session.dialect.split(&args.sql).len() > 1 {
        return Err(AppError::invalid("Run one statement at a time"));
    }
    let classification = session.dialect.classify(&args.sql);
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
    let executed = session
        .execute_in_transaction(&args.tab_id, &args.sql, &options)
        .await;
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
    let source = match &outcome {
        StatementOutcome::Rows(_) => result_source(&session, &args.tab_id, &args.sql).await,
        _ => None,
    };
    Ok(QueryResult {
        outcome: to_ipc(outcome),
        elapsed_ms,
        statement_class: classification.class,
        source,
    })
}

/// The table a plain single-table `SELECT` read. An unqualified name is resolved against the
/// tab's own connection, whose `USE` or `search_path` may differ from the session's.
async fn result_source(session: &Session, tab_id: &str, statement: &str) -> Option<TableRef> {
    let found = sql::single_table_source(statement, session.dialect.splitter_rules())?;
    let fold = |ident: sql::Ident| {
        if ident.quoted {
            ident.value
        } else {
            session.dialect.fold_identifier(&ident.value)
        }
    };
    let namespace = match found.schema {
        Some(schema) => NamespaceRef {
            catalog: None,
            schema: Some(fold(schema)),
        },
        // Editing is only offered when the table is known for sure; without the namespace the
        // result simply stays read-only.
        None => {
            let connection = session.tab_connection(tab_id).await.ok()?;
            connection.default_namespace().await.ok().flatten()?
        }
    };
    Some(TableRef {
        namespace,
        name: fold(found.table),
    })
}

/// The session's SSH tunnel dropped: the session is closed so it can be reopened cleanly once
/// the server is back (task 3.8).
pub(super) async fn tunnel_lost(state: &AppState, session_id: &str) -> AppError {
    // The session is unusable either way; a failure to close it changes nothing for the user.
    let _ = state.sessions.close(session_id).await;
    AppError::from(dbc_tunnel::TunnelError::Lost)
}

/// The plan for one statement, on the tab's connection, without running it (deferred
/// feature 3). Works on read-only connections: no `ANALYZE`, so nothing executes.
pub async fn query_explain(state: &AppState, args: QueryExplainArgs) -> Result<PlanNode, AppError> {
    if args.sql.trim().is_empty() {
        return Err(AppError::invalid("There is no statement to explain"));
    }
    let session = state.sessions.get(&args.session_id).await?;
    if !session.capabilities.explain {
        return Err(AppError::from(DbError::unsupported("explain")));
    }
    if session.dialect.split(&args.sql).len() > 1 {
        return Err(AppError::invalid("Explain one statement at a time"));
    }
    let statement = args.sql.clone();
    let planned = session
        .on_tab(&args.tab_id, |connection| async move {
            connection.explain(&statement).await
        })
        .await;
    match planned {
        Ok(plan) => Ok(plan),
        Err(_) if session.tunnel_lost() => Err(tunnel_lost(state, &session.id).await),
        Err(error) => Err(AppError::from_db(error, &args.sql)),
    }
}

/// The tab's transaction mode and whether a transaction is open (deferred feature 2).
pub async fn transaction_status(
    state: &AppState,
    args: TabArgs,
) -> Result<TransactionState, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    Ok(session.transaction(&args.tab_id))
}

/// Switches a tab between auto-commit and manual transactions. Leaving manual mode with a
/// transaction open is refused: the user commits or rolls back first.
pub async fn transaction_mode_set(
    state: &AppState,
    args: TransactionModeArgs,
) -> Result<TransactionState, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    if args.manual && session.capabilities.key_value {
        return Err(AppError::invalid(
            "Key-value connections have no manual transactions; use MULTI and EXEC",
        ));
    }
    session
        .set_manual(&args.tab_id, args.manual)
        .map_err(|e| AppError::invalid(e.message))
}

/// Commits or rolls back the tab's open transaction; without one, nothing happens.
pub async fn transaction_finish(
    state: &AppState,
    args: TransactionFinishArgs,
) -> Result<TransactionState, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    match session.finish_transaction(&args.tab_id, args.commit).await {
        Ok(next) => Ok(next),
        Err(_) if session.tunnel_lost() => Err(tunnel_lost(state, &session.id).await),
        Err(error) => Err(AppError::from(error)),
    }
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
