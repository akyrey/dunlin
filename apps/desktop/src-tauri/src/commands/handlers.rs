//! Tauri command wrappers. Each one only forwards to the implementation in `commands`.

use dbc_core::driver::DriverInfo;
use dbc_core::schema::{CompletionSchema, Namespace, ObjectSummary, TableInfo};
use dbc_store::ConnectionProfile;
use tauri::State;

use crate::commands;
use crate::error::AppError;
use crate::ipc::{
    ConnectionIdArgs, ConnectionSaveArgs, ConnectionTestArgs, ConnectionTestResult,
    HistoryListArgs, QueryExecuteArgs, QueryResult, SchemaObjectsArgs, SchemaTableArgs,
    SessionIdArgs, SessionInfo, SqlSplitArgs, SshKnownHostRemoveArgs, SshTrustHostArgs,
    StatementRange, TabArgs, TableSnippets,
};
use crate::state::AppState;

#[tauri::command]
pub fn driver_list(state: State<'_, AppState>) -> Vec<DriverInfo> {
    commands::driver_list(&state)
}

#[tauri::command]
pub async fn connection_list(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectionProfile>, AppError> {
    commands::connection_list(&state).await
}

#[tauri::command]
pub async fn connection_save(
    state: State<'_, AppState>,
    args: ConnectionSaveArgs,
) -> Result<ConnectionProfile, AppError> {
    commands::connection_save(&state, args).await
}

#[tauri::command]
pub async fn connection_delete(
    state: State<'_, AppState>,
    args: ConnectionIdArgs,
) -> Result<(), AppError> {
    commands::connection_delete(&state, args).await
}

#[tauri::command]
pub async fn connection_test(
    state: State<'_, AppState>,
    args: ConnectionTestArgs,
) -> Result<ConnectionTestResult, AppError> {
    commands::connection_test(&state, args).await
}

#[tauri::command]
pub async fn session_open(
    state: State<'_, AppState>,
    args: ConnectionIdArgs,
) -> Result<SessionInfo, AppError> {
    commands::session_open(&state, args).await
}

#[tauri::command]
pub async fn session_close(
    state: State<'_, AppState>,
    args: SessionIdArgs,
) -> Result<(), AppError> {
    commands::session_close(&state, args).await
}

#[tauri::command]
pub async fn sql_split(
    state: State<'_, AppState>,
    args: SqlSplitArgs,
) -> Result<Vec<StatementRange>, AppError> {
    commands::sql_split(&state, args).await
}

#[tauri::command]
pub async fn query_execute(
    state: State<'_, AppState>,
    args: QueryExecuteArgs,
) -> Result<QueryResult, AppError> {
    commands::query_execute(&state, args).await
}

#[tauri::command]
pub async fn query_cancel(state: State<'_, AppState>, args: TabArgs) -> Result<(), AppError> {
    commands::query_cancel(&state, args).await
}

#[tauri::command]
pub async fn tab_close(state: State<'_, AppState>, args: TabArgs) -> Result<(), AppError> {
    commands::tab_close(&state, args).await
}

#[tauri::command]
pub async fn schema_namespaces(
    state: State<'_, AppState>,
    args: SessionIdArgs,
) -> Result<Vec<Namespace>, AppError> {
    commands::schema_namespaces(&state, args).await
}

#[tauri::command]
pub async fn schema_objects(
    state: State<'_, AppState>,
    args: SchemaObjectsArgs,
) -> Result<Vec<ObjectSummary>, AppError> {
    commands::schema_objects(&state, args).await
}

#[tauri::command]
pub async fn schema_completion(
    state: State<'_, AppState>,
    args: SchemaObjectsArgs,
) -> Result<CompletionSchema, AppError> {
    commands::schema_completion(&state, args).await
}

#[tauri::command]
pub async fn schema_table(
    state: State<'_, AppState>,
    args: SchemaTableArgs,
) -> Result<TableInfo, AppError> {
    commands::schema_table(&state, args).await
}

#[tauri::command]
pub async fn schema_table_ddl(
    state: State<'_, AppState>,
    args: SchemaTableArgs,
) -> Result<String, AppError> {
    commands::schema_table_ddl(&state, args).await
}

#[tauri::command]
pub async fn schema_snippets(
    state: State<'_, AppState>,
    args: SchemaTableArgs,
) -> Result<TableSnippets, AppError> {
    commands::schema_snippets(&state, args).await
}

#[tauri::command]
pub async fn schema_refresh(
    state: State<'_, AppState>,
    args: SessionIdArgs,
) -> Result<(), AppError> {
    commands::schema_refresh(&state, args).await
}

#[tauri::command]
pub async fn history_list(
    state: State<'_, AppState>,
    args: HistoryListArgs,
) -> Result<Vec<dbc_store::HistoryEntry>, AppError> {
    commands::history_list(&state, args).await
}

#[tauri::command]
pub async fn history_clear(
    state: State<'_, AppState>,
    args: ConnectionIdArgs,
) -> Result<(), AppError> {
    commands::history_clear(&state, args).await
}

#[tauri::command]
pub async fn ssh_trust_host(
    state: State<'_, AppState>,
    args: SshTrustHostArgs,
) -> Result<(), AppError> {
    commands::ssh_trust_host(&state, args).await
}

#[tauri::command]
pub async fn ssh_known_hosts_list(
    state: State<'_, AppState>,
) -> Result<Vec<dbc_store::KnownHost>, AppError> {
    commands::ssh_known_hosts_list(&state).await
}

#[tauri::command]
pub async fn ssh_known_host_remove(
    state: State<'_, AppState>,
    args: SshKnownHostRemoveArgs,
) -> Result<(), AppError> {
    commands::ssh_known_host_remove(&state, args).await
}
