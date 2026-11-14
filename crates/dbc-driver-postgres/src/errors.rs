//! Maps `tokio-postgres` errors to [`DbError`] by SQLSTATE. Messages come from the server.

use std::error::Error as _;

use dbc_core::{DbError, DbErrorCode};
use tokio_postgres::error::{DbError as PgDbError, ErrorPosition};

/// An error while running `statement`; the server's 1-based character position becomes a
/// byte offset into it.
pub(crate) fn map_error(error: &tokio_postgres::Error, statement: &str) -> DbError {
    match error.as_db_error() {
        Some(db) => from_server(db, Some(statement)),
        None => transport(error),
    }
}

/// An error while connecting or authenticating.
pub(crate) fn map_connect_error(error: &tokio_postgres::Error) -> DbError {
    match error.as_db_error() {
        Some(db) => {
            let mapped = from_server(db, None);
            match mapped.code {
                DbErrorCode::Auth | DbErrorCode::Tls => mapped,
                _ => DbError {
                    code: DbErrorCode::Connect,
                    ..mapped
                },
            }
        }
        None => {
            let mapped = transport(error);
            match mapped.code {
                DbErrorCode::ConnectionLost => DbError {
                    code: DbErrorCode::Connect,
                    ..mapped
                },
                _ => mapped,
            }
        }
    }
}

fn from_server(db: &PgDbError, statement: Option<&str>) -> DbError {
    let sqlstate = db.code().code();
    let mut error = DbError::new(code_for(sqlstate), db.message()).with_sqlstate(sqlstate);
    let detail = [db.detail(), db.hint()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
    if !detail.is_empty() {
        error = error.with_detail(detail);
    }
    if let (Some(ErrorPosition::Original(position)), Some(statement)) = (db.position(), statement)
        && let Some(offset) = char_to_byte(statement, *position)
    {
        error = error.with_position(offset);
    }
    error
}

pub(crate) fn code_for(sqlstate: &str) -> DbErrorCode {
    match sqlstate {
        "28000" | "28P01" => DbErrorCode::Auth,
        "57014" => DbErrorCode::Cancelled,
        "25006" => DbErrorCode::ReadOnly,
        "42501" => DbErrorCode::Permission,
        "57P01" | "57P02" | "57P03" => DbErrorCode::ConnectionLost,
        "3D000" => DbErrorCode::Connect,
        _ if sqlstate.starts_with("23") => DbErrorCode::Constraint,
        _ if sqlstate.starts_with("42") => DbErrorCode::Syntax,
        _ if sqlstate.starts_with("08") => DbErrorCode::ConnectionLost,
        _ => DbErrorCode::Execution,
    }
}

/// Errors that did not come from the server: closed connection, I/O, TLS, timeouts.
fn transport(error: &tokio_postgres::Error) -> DbError {
    if error.is_closed() {
        return DbError::new(
            DbErrorCode::ConnectionLost,
            "The connection to the server was lost",
        );
    }
    let mut source = error.source();
    while let Some(cause) = source {
        if cause.downcast_ref::<rustls::Error>().is_some() {
            return DbError::new(DbErrorCode::Tls, format!("TLS error: {cause}"));
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            if io.kind() == std::io::ErrorKind::TimedOut {
                return DbError::new(DbErrorCode::Timeout, "The server did not respond in time");
            }
            if let Some(inner) = io.get_ref()
                && inner.downcast_ref::<rustls::Error>().is_some()
            {
                return DbError::new(DbErrorCode::Tls, format!("TLS error: {inner}"));
            }
        }
        source = cause.source();
    }
    DbError::new(DbErrorCode::Connect, error.to_string())
}

/// Byte offset of a 1-based character position.
fn char_to_byte(text: &str, position: u32) -> Option<usize> {
    let index = usize::try_from(position).ok()?.checked_sub(1)?;
    text.char_indices()
        .nth(index)
        .map(|(offset, _)| offset)
        .or_else(|| (index == text.chars().count()).then_some(text.len()))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("28P01", DbErrorCode::Auth)]
    #[case("57014", DbErrorCode::Cancelled)]
    #[case("25006", DbErrorCode::ReadOnly)]
    #[case("42501", DbErrorCode::Permission)]
    #[case("42601", DbErrorCode::Syntax)]
    #[case("42P01", DbErrorCode::Syntax)]
    #[case("23505", DbErrorCode::Constraint)]
    #[case("23503", DbErrorCode::Constraint)]
    #[case("08006", DbErrorCode::ConnectionLost)]
    #[case("57P01", DbErrorCode::ConnectionLost)]
    #[case("3D000", DbErrorCode::Connect)]
    #[case("22012", DbErrorCode::Execution)]
    fn maps_sqlstates(#[case] sqlstate: &str, #[case] expected: DbErrorCode) {
        assert_eq!(code_for(sqlstate), expected);
    }

    #[rstest]
    #[case("SELECT x", 8, Some(7))]
    // `F` is the 12th character; `é` takes two bytes, so it starts at byte 12.
    #[case("SELECT 'é' FRM", 12, Some(12))]
    #[case("abc", 4, Some(3))]
    #[case("abc", 0, None)]
    #[case("abc", 9, None)]
    fn converts_character_positions_to_byte_offsets(
        #[case] text: &str,
        #[case] position: u32,
        #[case] expected: Option<usize>,
    ) {
        assert_eq!(char_to_byte(text, position), expected);
    }
}
