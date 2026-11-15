//! `RedisError` → `DbError`.

use dbc_core::{DbError, DbErrorCode};
use redis::{ErrorKind, RedisError};

pub fn map_error(error: &RedisError) -> DbError {
    let code = match error.code() {
        Some("NOAUTH" | "WRONGPASS") => DbErrorCode::Auth,
        Some("NOPERM") => DbErrorCode::Permission,
        Some("READONLY") => DbErrorCode::ReadOnly,
        Some(_)
            if error
                .detail()
                .is_some_and(|d| d.starts_with("unknown command")) =>
        {
            DbErrorCode::Syntax
        }
        Some(_) => DbErrorCode::Execution,
        None if error.kind() == ErrorKind::AuthenticationFailed => DbErrorCode::Auth,
        None if error.is_timeout() => DbErrorCode::Timeout,
        None if error.is_connection_dropped() || error.is_io_error() => DbErrorCode::ConnectionLost,
        None => DbErrorCode::Internal,
    };
    DbError::new(code, error.to_string())
}

/// While connecting, a dropped socket means the server could not be reached.
pub fn map_connect_error(error: &RedisError) -> DbError {
    let mapped = map_error(error);
    if mapped.code == DbErrorCode::ConnectionLost {
        return DbError::new(DbErrorCode::Connect, mapped.message);
    }
    mapped
}
