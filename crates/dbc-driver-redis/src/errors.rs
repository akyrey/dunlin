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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;

    fn server(line: &str) -> RedisError {
        match redis::parse_redis_value(format!("-{line}\r\n").as_bytes()).unwrap() {
            redis::Value::ServerError(error) => RedisError::from(error),
            other => panic!("not an error: {other:?}"),
        }
    }

    #[test]
    fn server_errors_keep_their_meaning() {
        assert_eq!(map_error(&server("WRONGPASS bad")).code, DbErrorCode::Auth);
        assert_eq!(
            map_error(&server("NOPERM no")).code,
            DbErrorCode::Permission
        );
        assert_eq!(
            map_error(&server("READONLY replica")).code,
            DbErrorCode::ReadOnly
        );
        assert_eq!(
            map_error(&server("ERR unknown command 'X'")).code,
            DbErrorCode::Syntax
        );
        assert_eq!(
            map_error(&server("ERR syntax error")).code,
            DbErrorCode::Execution
        );
    }

    #[test]
    fn client_side_errors_are_classified() {
        let auth = RedisError::from((ErrorKind::AuthenticationFailed, "auth"));
        let io = RedisError::from(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "reset",
        ));
        let timeout = RedisError::from(std::io::Error::new(std::io::ErrorKind::TimedOut, "slow"));
        let other = RedisError::from((ErrorKind::Client, "confused"));

        assert_eq!(map_error(&auth).code, DbErrorCode::Auth);
        assert_eq!(map_error(&io).code, DbErrorCode::ConnectionLost);
        assert_eq!(map_error(&timeout).code, DbErrorCode::Timeout);
        assert_eq!(map_error(&other).code, DbErrorCode::Internal);
    }

    #[test]
    fn a_dropped_socket_while_connecting_is_a_connect_error() {
        let io = RedisError::from(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "no",
        ));

        assert_eq!(map_connect_error(&io).code, DbErrorCode::Connect);
        assert_eq!(
            map_connect_error(&server("WRONGPASS bad")).code,
            DbErrorCode::Auth
        );
    }
}
