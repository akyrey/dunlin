//! Command implementations as plain async functions over [`AppState`], so they can be tested
//! without a running app. `handlers` wraps them as Tauri commands.

pub(crate) mod connections;
pub(crate) mod handlers;
mod queries;
mod schema;
mod sessions;
mod ssh;

pub use connections::{
    connection_delete, connection_list, connection_save, connection_test, driver_list,
};
pub use queries::{MAX_ROW_LIMIT, query_cancel, query_execute, sql_split};
pub use schema::{
    schema_namespaces, schema_objects, schema_refresh, schema_table, schema_table_ddl,
};
pub use sessions::{session_close, session_open, tab_close};
pub use ssh::{ssh_known_host_remove, ssh_known_hosts_list, ssh_trust_host};
