//! Dunlin desktop backend: Tauri commands over the session manager.

pub mod commands;
pub mod connect;
pub mod error;
pub mod ipc;
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

/// Starts the app. Returns an error instead of panicking if setup fails.
pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let secrets = std::sync::Arc::new(dbc_store::KeychainSecretStore::open()?);
            let state = AppState::open(&dir.join(STORE_FILE), secrets)?;
            app.manage(state);
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
            handlers::ssh_trust_host,
            handlers::ssh_known_hosts_list,
            handlers::ssh_known_host_remove,
        ])
        .run(tauri::generate_context!())
}
