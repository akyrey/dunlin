//! Dunlin desktop backend: Tauri commands over the session manager.

pub mod commands;
pub mod connect;
pub mod error;
pub mod ipc;
pub mod logging;
pub mod profile;
pub mod registry;
pub mod session;
pub mod ssh;
pub mod state;

#[cfg(test)]
mod bindings;

use tauri::Manager;

use crate::commands::handlers;
use crate::state::AppState;

const STORE_FILE: &str = "dunlin.db";

struct LogGuard {
    _guard: tracing_appender::non_blocking::WorkerGuard,
}

/// Starts the app. Returns an error instead of panicking if setup fails.
pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // Size, position and maximised or full screen state (task 8.12).
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED
                        | tauri_plugin_window_state::StateFlags::FULLSCREEN,
                )
                .build(),
        )
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let secrets = std::sync::Arc::new(dbc_store::KeychainSecretStore::open()?);
            let state = AppState::open(&dir.join(STORE_FILE), secrets)?;
            app.manage(state);
            let logs = app.path().app_log_dir()?;
            std::fs::create_dir_all(&logs)?;
            let guard = logging::init(&logs).map_err(|e| std::io::Error::other(e.to_string()))?;
            // Kept for the life of the app so buffered lines are written.
            app.manage(LogGuard { _guard: guard });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            handlers::driver_list,
            handlers::connection_list,
            handlers::connection_save,
            handlers::connection_delete,
            handlers::connection_test,
            handlers::session_open,
            handlers::session_close,
            handlers::sql_split,
            handlers::query_execute,
            handlers::query_cancel,
            handlers::tab_close,
            handlers::schema_namespaces,
            handlers::schema_objects,
            handlers::schema_completion,
            handlers::schema_table,
            handlers::schema_table_ddl,
            handlers::schema_refresh,
            handlers::schema_snippets,
            handlers::table_page,
            handlers::table_count,
            handlers::cell_full_value,
            handlers::changes_preview,
            handlers::changes_apply,
            handlers::export_text,
            handlers::export_result,
            handlers::export_table,
            handlers::saved_query_list,
            handlers::saved_query_save,
            handlers::saved_query_delete,
            handlers::settings_get,
            handlers::settings_set,
            handlers::logs_dir,
            handlers::history_list,
            handlers::history_clear,
            handlers::ssh_trust_host,
            handlers::ssh_known_hosts_list,
            handlers::ssh_known_host_remove,
        ])
        .run(tauri::generate_context!())
}
