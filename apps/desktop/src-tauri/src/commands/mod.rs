//! Command implementations as plain async functions over [`AppState`], so they can be tested
//! without a running app. `handlers` wraps them as Tauri commands.

mod changes;
pub(crate) mod connections;
mod export;
pub(crate) mod handlers;
mod history;
mod queries;
mod schema;
mod sessions;
mod ssh;
mod table_data;

pub use changes::{changes_apply, changes_preview};
pub use connections::{
    connection_delete, connection_list, connection_save, connection_test, driver_list,
};
pub use export::{export_result, export_table, export_text};
pub use history::{history_clear, history_list};
pub use queries::{MAX_ROW_LIMIT, query_cancel, query_execute, sql_split};
pub use schema::{
    schema_completion, schema_namespaces, schema_objects, schema_refresh, schema_snippets,
    schema_table, schema_table_ddl,
};
pub use sessions::{session_close, session_open, tab_close};
pub use ssh::{ssh_known_host_remove, ssh_known_hosts_list, ssh_trust_host};
pub use table_data::{PAGE_DISPLAY_LIMIT, cell_full_value, table_count, table_page};
