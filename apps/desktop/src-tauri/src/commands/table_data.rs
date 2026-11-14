//! Table data commands (task 6.4): a page, a row count and one full cell value, built by
//! `dbc_core::data` and run on the view's own connection (D-022).

use std::time::Instant;

use dbc_core::data::{self, KeyValue, RowCount, TablePageRequest};
use dbc_core::{DbError, Value, WireValue};

use crate::error::AppError;
use crate::ipc::{CellFullValueArgs, KeyArg, TableCountArgs, TablePage, TablePageArgs};
use crate::session::Session;
use crate::state::AppState;

/// Cell values are cut to this size in pages; `cell_full_value` fetches the rest.
pub const PAGE_DISPLAY_LIMIT: usize = dbc_core::DEFAULT_DISPLAY_LIMIT;

fn keys(key: Vec<KeyArg>) -> Result<Vec<KeyValue>, AppError> {
    key.into_iter()
        .map(|KeyArg { column, value }| match Value::try_from(value) {
            Ok(value) => Ok(KeyValue { column, value }),
            Err(e) => Err(AppError::invalid(format!("Key value for {column}: {e}"))),
        })
        .collect()
}

pub(super) async fn finish<T>(
    state: &AppState,
    session: &Session,
    result: Result<T, DbError>,
) -> Result<T, AppError> {
    match result {
        Ok(value) => Ok(value),
        Err(_) if session.tunnel_lost() => {
            Err(super::queries::tunnel_lost(state, &session.id).await)
        }
        Err(error) => Err(AppError::from(error)),
    }
}

pub async fn table_page(state: &AppState, args: TablePageArgs) -> Result<TablePage, AppError> {
    if !(1..=data::MAX_PAGE_SIZE).contains(&args.limit) {
        return Err(AppError::invalid(format!(
            "The page size must be between 1 and {}",
            data::MAX_PAGE_SIZE
        )));
    }
    let session = state.sessions.get(&args.session_id).await?;
    let info = session.schema_table(&args.table).await;
    let info = finish(state, &session, info).await?;
    // One row more than the page tells whether another page follows.
    let request = TablePageRequest {
        table: args.table,
        filter: args.filter,
        key: keys(args.key)?,
        sort: args.sort,
        limit: args.limit + 1,
        offset: args.offset,
    };
    let started = Instant::now();
    let dialect = std::sync::Arc::clone(&session.dialect);
    let result = session
        .on_tab(&args.view_id, |connection| async move {
            data::fetch_page(connection.as_ref(), dialect.as_ref(), &info, &request).await
        })
        .await;
    let mut page = finish(state, &session, result).await?;
    let has_more = page.rows.len() > args.limit;
    page.rows.truncate(args.limit);
    Ok(TablePage {
        columns: page.columns,
        rows: page
            .rows
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|v| v.into_wire(PAGE_DISPLAY_LIMIT))
                    .collect()
            })
            .collect(),
        has_more,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

pub async fn table_count(state: &AppState, args: TableCountArgs) -> Result<RowCount, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let info = session.schema_table(&args.table).await;
    let info = finish(state, &session, info).await?;
    let key = keys(args.key)?;
    let dialect = std::sync::Arc::clone(&session.dialect);
    let filter = args.filter;
    let exact = args.exact;
    let result = session
        .on_tab(&args.view_id, |connection| async move {
            data::count_rows(
                connection.as_ref(),
                dialect.as_ref(),
                &info,
                filter.as_deref(),
                &key,
                exact,
            )
            .await
        })
        .await;
    finish(state, &session, result).await
}

/// The whole value of a cell that the page truncated, addressed by the row key (task 6.11).
pub async fn cell_full_value(
    state: &AppState,
    args: CellFullValueArgs,
) -> Result<WireValue, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let info = session.schema_table(&args.table).await;
    let info = finish(state, &session, info).await?;
    let key = keys(args.key)?;
    let dialect = std::sync::Arc::clone(&session.dialect);
    let column = args.column;
    let result = session
        .on_tab(&args.view_id, |connection| async move {
            data::fetch_cell(connection.as_ref(), dialect.as_ref(), &info, &key, &column).await
        })
        .await;
    Ok(finish(state, &session, result).await?.into_wire(usize::MAX))
}
