//! Command implementations as plain async functions over [`AppState`], so they can be tested
//! without a running app. `handlers` wraps them as Tauri commands.

pub(crate) mod connections;
pub(crate) mod handlers;
mod queries;
mod sessions;

pub use connections::{
    connection_delete, connection_list, connection_save, connection_test, driver_list,
};
pub use queries::{MAX_ROW_LIMIT, query_cancel, query_execute, sql_split};
pub use sessions::{session_close, session_open, tab_close};
