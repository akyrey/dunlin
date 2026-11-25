//! Schema browser commands (task 4.7). They read through the session's metadata connection
//! and its cache; `schema_refresh` clears it.

use dbc_core::DbError;
use dbc_core::schema::{CompletionSchema, Namespace, ObjectSummary, TableInfo};

use crate::error::AppError;
use crate::ipc::{SchemaObjectsArgs, SchemaTableArgs, SessionIdArgs, TableSnippets};
use crate::session::Session;
use crate::state::AppState;

pub async fn schema_namespaces(
    state: &AppState,
    args: SessionIdArgs,
) -> Result<Vec<Namespace>, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let result = session.schema_namespaces().await;
    finish(state, &session, result).await
}

pub async fn schema_objects(
    state: &AppState,
    args: SchemaObjectsArgs,
) -> Result<Vec<ObjectSummary>, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let result = session.schema_objects(&args.namespace).await;
    finish(state, &session, result).await
}

/// Table and column names of one namespace, for editor autocomplete (task 5.2).
pub async fn schema_completion(
    state: &AppState,
    args: SchemaObjectsArgs,
) -> Result<CompletionSchema, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let result = session.schema_completion(&args.namespace).await;
    finish(state, &session, result).await
}

pub async fn schema_table(state: &AppState, args: SchemaTableArgs) -> Result<TableInfo, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let result = session.schema_table(&args.table).await;
    finish(state, &session, result).await
}

pub async fn schema_table_ddl(state: &AppState, args: SchemaTableArgs) -> Result<String, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let result = session.schema_table_ddl(&args.table).await;
    finish(state, &session, result).await
}

/// The qualified name and a `SELECT` listing every column, for the tree's context actions.
pub async fn schema_snippets(
    state: &AppState,
    args: SchemaTableArgs,
) -> Result<TableSnippets, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let result = session.schema_table(&args.table).await;
    let info = finish(state, &session, result).await?;
    let dialect = &session.dialect;
    let qualified_name = dialect.qualified_name(&args.table);
    let columns: Vec<String> = info
        .columns
        .iter()
        .map(|c| dialect.quote_identifier(&c.name))
        .collect();
    let list = if columns.is_empty() {
        "*".to_owned()
    } else {
        columns.join(", ")
    };
    Ok(TableSnippets {
        select: format!("SELECT {list}\nFROM {qualified_name};"),
        qualified_name,
    })
}

pub async fn schema_refresh(state: &AppState, args: SessionIdArgs) -> Result<(), AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    session.invalidate_schema();
    Ok(())
}

/// A failure caused by a dropped SSH tunnel closes the session, as for queries (task 3.8).
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
