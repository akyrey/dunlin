//! Tauri command wrappers. Each one only forwards to the implementation in `commands`.

use dbc_core::WireValue;
use dbc_core::data::RowCount;
use dbc_core::driver::DriverInfo;
use dbc_core::schema::{CompletionSchema, Namespace, ObjectSummary, TableInfo};
use dbc_store::ConnectionProfile;
use tauri::State;

use crate::commands;
use crate::error::AppError;
use crate::ipc::{
    ApplyResult, CellFullValueArgs, ChangesArgs, TableCountArgs, TablePage, TablePageArgs,
};
use crate::ipc::{
    ConnectionIdArgs, ConnectionSaveArgs, ConnectionTestArgs, ConnectionTestResult,
    HistoryListArgs, QueryExecuteArgs, QueryResult, SchemaObjectsArgs, SchemaTableArgs,
    SessionIdArgs, SessionInfo, SqlSplitArgs, SshKnownHostRemoveArgs, SshTrustHostArgs,
    StatementRange, TabArgs, TableSnippets,
};
use crate::ipc::{ExportDone, ExportProgress, ExportResultArgs, ExportTableArgs, ExportTextArgs};
use crate::ipc::{QueryExplainArgs, TransactionFinishArgs, TransactionModeArgs};
use crate::ipc::{SavedQueryIdArgs, SavedQuerySaveArgs, SettingsSetArgs};
use crate::logging::logged;
use crate::session::TransactionState;
use crate::state::AppState;

#[tauri::command]
pub fn driver_list(state: State<'_, AppState>) -> Vec<DriverInfo> {
    commands::driver_list(&state)
}

#[tauri::command]
pub async fn connection_list(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectionProfile>, AppError> {
    logged("connection_list", commands::connection_list(&state)).await
}

#[tauri::command]
pub async fn connection_save(
    state: State<'_, AppState>,
    args: ConnectionSaveArgs,
) -> Result<ConnectionProfile, AppError> {
    logged("connection_save", commands::connection_save(&state, args)).await
}

#[tauri::command]
pub async fn connection_delete(
    state: State<'_, AppState>,
    args: ConnectionIdArgs,
) -> Result<(), AppError> {
    logged(
        "connection_delete",
        commands::connection_delete(&state, args),
    )
    .await
}

#[tauri::command]
pub async fn connection_duplicate(
    state: State<'_, AppState>,
    args: ConnectionIdArgs,
) -> Result<ConnectionProfile, AppError> {
    logged(
        "connection_duplicate",
        commands::connection_duplicate(&state, args),
    )
    .await
}

#[tauri::command]
pub async fn connection_reorder(
    state: State<'_, AppState>,
    args: crate::ipc::ConnectionReorderArgs,
) -> Result<Vec<ConnectionProfile>, AppError> {
    logged(
        "connection_reorder",
        commands::connection_reorder(&state, args),
    )
    .await
}

#[tauri::command]
pub async fn connection_test(
    state: State<'_, AppState>,
    args: ConnectionTestArgs,
) -> Result<ConnectionTestResult, AppError> {
    logged("connection_test", commands::connection_test(&state, args)).await
}

#[tauri::command]
pub async fn session_open(
    state: State<'_, AppState>,
    args: ConnectionIdArgs,
) -> Result<SessionInfo, AppError> {
    logged("session_open", commands::session_open(&state, args)).await
}

#[tauri::command]
pub async fn session_close(
    state: State<'_, AppState>,
    args: SessionIdArgs,
) -> Result<(), AppError> {
    logged("session_close", commands::session_close(&state, args)).await
}

#[tauri::command]
pub async fn sql_split(
    state: State<'_, AppState>,
    args: SqlSplitArgs,
) -> Result<Vec<StatementRange>, AppError> {
    logged("sql_split", commands::sql_split(&state, args)).await
}

#[tauri::command]
pub async fn query_execute(
    state: State<'_, AppState>,
    args: QueryExecuteArgs,
) -> Result<QueryResult, AppError> {
    logged("query_execute", commands::query_execute(&state, args)).await
}

#[tauri::command]
pub async fn query_cancel(state: State<'_, AppState>, args: TabArgs) -> Result<(), AppError> {
    logged("query_cancel", commands::query_cancel(&state, args)).await
}

#[tauri::command]
pub async fn connections_export(
    state: State<'_, AppState>,
    args: crate::ipc::ConnectionsExportArgs,
) -> Result<usize, AppError> {
    logged(
        "connections_export",
        commands::connections_export(&state, args),
    )
    .await
}

#[tauri::command]
pub async fn connections_import(
    state: State<'_, AppState>,
    args: crate::ipc::ConnectionsImportArgs,
) -> Result<crate::ipc::ImportReport, AppError> {
    logged(
        "connections_import",
        commands::connections_import(&state, args),
    )
    .await
}

#[tauri::command]
pub async fn ssh_config_hosts() -> Result<Vec<crate::ipc::SshConfigEntry>, AppError> {
    logged("ssh_config_hosts", commands::ssh_config_hosts()).await
}

#[tauri::command]
pub async fn query_explain(
    state: State<'_, AppState>,
    args: QueryExplainArgs,
) -> Result<dbc_core::plan::PlanNode, AppError> {
    logged("query_explain", commands::query_explain(&state, args)).await
}

#[tauri::command]
pub async fn transaction_status(
    state: State<'_, AppState>,
    args: TabArgs,
) -> Result<TransactionState, AppError> {
    logged(
        "transaction_status",
        commands::transaction_status(&state, args),
    )
    .await
}

#[tauri::command]
pub async fn transaction_mode_set(
    state: State<'_, AppState>,
    args: TransactionModeArgs,
) -> Result<TransactionState, AppError> {
    logged(
        "transaction_mode_set",
        commands::transaction_mode_set(&state, args),
    )
    .await
}

#[tauri::command]
pub async fn transaction_finish(
    state: State<'_, AppState>,
    args: TransactionFinishArgs,
) -> Result<TransactionState, AppError> {
    logged(
        "transaction_finish",
        commands::transaction_finish(&state, args),
    )
    .await
}

#[tauri::command]
pub async fn tab_close(state: State<'_, AppState>, args: TabArgs) -> Result<(), AppError> {
    logged("tab_close", commands::tab_close(&state, args)).await
}

#[tauri::command]
pub async fn schema_namespaces(
    state: State<'_, AppState>,
    args: SessionIdArgs,
) -> Result<Vec<Namespace>, AppError> {
    logged(
        "schema_namespaces",
        commands::schema_namespaces(&state, args),
    )
    .await
}

#[tauri::command]
pub async fn schema_objects(
    state: State<'_, AppState>,
    args: SchemaObjectsArgs,
) -> Result<Vec<ObjectSummary>, AppError> {
    logged("schema_objects", commands::schema_objects(&state, args)).await
}

#[tauri::command]
pub async fn schema_completion(
    state: State<'_, AppState>,
    args: SchemaObjectsArgs,
) -> Result<CompletionSchema, AppError> {
    logged(
        "schema_completion",
        commands::schema_completion(&state, args),
    )
    .await
}

#[tauri::command]
pub async fn schema_table(
    state: State<'_, AppState>,
    args: SchemaTableArgs,
) -> Result<TableInfo, AppError> {
    logged("schema_table", commands::schema_table(&state, args)).await
}

#[tauri::command]
pub async fn schema_table_ddl(
    state: State<'_, AppState>,
    args: SchemaTableArgs,
) -> Result<String, AppError> {
    logged("schema_table_ddl", commands::schema_table_ddl(&state, args)).await
}

#[tauri::command]
pub async fn schema_snippets(
    state: State<'_, AppState>,
    args: SchemaTableArgs,
) -> Result<TableSnippets, AppError> {
    logged("schema_snippets", commands::schema_snippets(&state, args)).await
}

#[tauri::command]
pub async fn schema_refresh(
    state: State<'_, AppState>,
    args: SessionIdArgs,
) -> Result<(), AppError> {
    logged("schema_refresh", commands::schema_refresh(&state, args)).await
}

#[tauri::command]
pub async fn table_page(
    state: State<'_, AppState>,
    args: TablePageArgs,
) -> Result<TablePage, AppError> {
    logged("table_page", commands::table_page(&state, args)).await
}

#[tauri::command]
pub async fn table_count(
    state: State<'_, AppState>,
    args: TableCountArgs,
) -> Result<RowCount, AppError> {
    logged("table_count", commands::table_count(&state, args)).await
}

#[tauri::command]
pub async fn cell_full_value(
    state: State<'_, AppState>,
    args: CellFullValueArgs,
) -> Result<WireValue, AppError> {
    logged("cell_full_value", commands::cell_full_value(&state, args)).await
}

#[tauri::command]
pub async fn changes_preview(
    state: State<'_, AppState>,
    args: ChangesArgs,
) -> Result<Vec<String>, AppError> {
    logged("changes_preview", commands::changes_preview(&state, args)).await
}

#[tauri::command]
pub async fn changes_apply(
    state: State<'_, AppState>,
    args: ChangesArgs,
) -> Result<ApplyResult, AppError> {
    logged("changes_apply", commands::changes_apply(&state, args)).await
}

#[tauri::command]
pub async fn export_text(
    state: State<'_, AppState>,
    args: ExportTextArgs,
) -> Result<String, AppError> {
    logged("export_text", commands::export_text(&state, args)).await
}

#[tauri::command]
pub async fn export_result(
    state: State<'_, AppState>,
    args: ExportResultArgs,
) -> Result<ExportDone, AppError> {
    logged("export_result", commands::export_result(&state, args)).await
}

#[tauri::command]
pub async fn export_table(
    state: State<'_, AppState>,
    args: ExportTableArgs,
    on_progress: tauri::ipc::Channel<ExportProgress>,
) -> Result<ExportDone, AppError> {
    let progress = move |rows: u64| {
        // A closed channel means the window went away; the export itself carries on.
        let _ = on_progress.send(ExportProgress {
            rows_written: rows.to_string(),
        });
    };
    logged(
        "export_table",
        commands::export_table(&state, args, progress),
    )
    .await
}

#[tauri::command]
pub async fn saved_query_list(
    state: State<'_, AppState>,
) -> Result<Vec<dbc_store::SavedQuery>, AppError> {
    logged("saved_query_list", commands::saved_query_list(&state)).await
}

#[tauri::command]
pub async fn saved_query_save(
    state: State<'_, AppState>,
    args: SavedQuerySaveArgs,
) -> Result<dbc_store::SavedQuery, AppError> {
    logged("saved_query_save", commands::saved_query_save(&state, args)).await
}

#[tauri::command]
pub async fn saved_query_delete(
    state: State<'_, AppState>,
    args: SavedQueryIdArgs,
) -> Result<(), AppError> {
    logged(
        "saved_query_delete",
        commands::saved_query_delete(&state, args),
    )
    .await
}

/// The frontend's first usable render; returns the cold start time in milliseconds.
#[tauri::command]
pub fn app_ready() -> u64 {
    crate::logging::ready()
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> dbc_store::Settings {
    commands::settings_get(&state)
}

#[tauri::command]
pub async fn settings_set(
    state: State<'_, AppState>,
    args: SettingsSetArgs,
) -> Result<dbc_store::Settings, AppError> {
    logged("settings_set", commands::settings_set(&state, args)).await
}

/// The folder the log files are written to (task 8.18).
#[tauri::command]
pub fn logs_dir(app: tauri::AppHandle) -> Result<String, AppError> {
    use tauri::Manager;
    app.path()
        .app_log_dir()
        .map(|dir| dir.to_string_lossy().into_owned())
        .map_err(|e| AppError::internal(format!("The log folder is unknown: {e}")))
}

#[tauri::command]
pub async fn history_list(
    state: State<'_, AppState>,
    args: HistoryListArgs,
) -> Result<Vec<dbc_store::HistoryEntry>, AppError> {
    logged("history_list", commands::history_list(&state, args)).await
}

#[tauri::command]
pub async fn history_clear(
    state: State<'_, AppState>,
    args: ConnectionIdArgs,
) -> Result<(), AppError> {
    logged("history_clear", commands::history_clear(&state, args)).await
}

#[tauri::command]
pub async fn ssh_trust_host(
    state: State<'_, AppState>,
    args: SshTrustHostArgs,
) -> Result<(), AppError> {
    logged("ssh_trust_host", commands::ssh_trust_host(&state, args)).await
}

#[tauri::command]
pub async fn ssh_known_hosts_list(
    state: State<'_, AppState>,
) -> Result<Vec<dbc_store::KnownHost>, AppError> {
    logged(
        "ssh_known_hosts_list",
        commands::ssh_known_hosts_list(&state),
    )
    .await
}

#[tauri::command]
pub async fn ssh_known_host_remove(
    state: State<'_, AppState>,
    args: SshKnownHostRemoveArgs,
) -> Result<(), AppError> {
    logged(
        "ssh_known_host_remove",
        commands::ssh_known_host_remove(&state, args),
    )
    .await
}
