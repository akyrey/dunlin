//! Maps `rusqlite` errors to [`DbError`]. Messages come from SQLite and contain no secrets.

use dbc_core::{DbError, DbErrorCode};
use rusqlite::ErrorCode;

pub(crate) fn map_error(error: rusqlite::Error) -> DbError {
    match error {
        rusqlite::Error::SqlInputError {
            error, msg, offset, ..
        } => {
            let mapped = DbError::new(code_for(error.code), msg);
            match usize::try_from(offset) {
                Ok(position) => mapped.with_position(position),
                Err(_) => mapped,
            }
        }
        rusqlite::Error::SqliteFailure(error, msg) => {
            let message = msg.unwrap_or_else(|| error.to_string());
            DbError::new(code_for(error.code), message)
        }
        rusqlite::Error::MultipleStatement => DbError::new(
            DbErrorCode::Syntax,
            "Only one statement can be executed at a time",
        ),
        other => DbError::new(DbErrorCode::Internal, other.to_string()),
    }
}

/// Like [`map_error`], but failures to open or recognise the file are connection errors.
pub(crate) fn map_open_error(error: rusqlite::Error) -> DbError {
    let mapped = map_error(error);
    match mapped.code {
        DbErrorCode::Internal | DbErrorCode::Syntax => DbError {
            code: DbErrorCode::Connect,
            ..mapped
        },
        _ => mapped,
    }
}

fn code_for(code: ErrorCode) -> DbErrorCode {
    match code {
        ErrorCode::ReadOnly => DbErrorCode::ReadOnly,
        ErrorCode::ConstraintViolation => DbErrorCode::Constraint,
        ErrorCode::OperationInterrupted => DbErrorCode::Cancelled,
        ErrorCode::CannotOpen | ErrorCode::NotADatabase | ErrorCode::NotFound => {
            DbErrorCode::Connect
        }
        ErrorCode::PermissionDenied | ErrorCode::AuthorizationForStatementDenied => {
            DbErrorCode::Permission
        }
        ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => DbErrorCode::Timeout,
        // SQLITE_ERROR covers syntax errors and unknown tables or columns, like SQLSTATE class 42.
        ErrorCode::Unknown => DbErrorCode::Syntax,
        _ => DbErrorCode::Internal,
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use rusqlite::ffi;

    use super::*;

    fn failure(code: i32) -> rusqlite::Error {
        rusqlite::Error::SqliteFailure(ffi::Error::new(code), Some("boom".to_owned()))
    }

    #[rstest]
    #[case(ffi::SQLITE_READONLY, DbErrorCode::ReadOnly)]
    #[case(ffi::SQLITE_CONSTRAINT, DbErrorCode::Constraint)]
    #[case(ffi::SQLITE_INTERRUPT, DbErrorCode::Cancelled)]
    #[case(ffi::SQLITE_CANTOPEN, DbErrorCode::Connect)]
    #[case(ffi::SQLITE_NOTADB, DbErrorCode::Connect)]
    #[case(ffi::SQLITE_PERM, DbErrorCode::Permission)]
    #[case(ffi::SQLITE_AUTH, DbErrorCode::Permission)]
    #[case(ffi::SQLITE_BUSY, DbErrorCode::Timeout)]
    #[case(ffi::SQLITE_ERROR, DbErrorCode::Syntax)]
    #[case(ffi::SQLITE_IOERR, DbErrorCode::Internal)]
    fn maps_sqlite_codes(#[case] code: i32, #[case] expected: DbErrorCode) {
        let mapped = map_error(failure(code));

        assert_eq!(mapped.code, expected);
        assert_eq!(mapped.message, "boom");
    }

    #[test]
    fn input_errors_keep_their_offset() {
        let error = rusqlite::Error::SqlInputError {
            error: ffi::Error::new(ffi::SQLITE_ERROR),
            msg: "near \"FRM\": syntax error".to_owned(),
            sql: "SELECT * FRM t".to_owned(),
            offset: 9,
        };

        let mapped = map_error(error);

        assert_eq!(
            (mapped.code, mapped.position),
            (DbErrorCode::Syntax, Some(9))
        );
    }

    #[test]
    fn negative_offset_means_no_position() {
        let error = rusqlite::Error::SqlInputError {
            error: ffi::Error::new(ffi::SQLITE_ERROR),
            msg: "x".to_owned(),
            sql: String::new(),
            offset: -1,
        };

        assert_eq!(map_error(error).position, None);
    }

    #[test]
    fn multiple_statements_are_a_syntax_error() {
        assert_eq!(
            map_error(rusqlite::Error::MultipleStatement).code,
            DbErrorCode::Syntax
        );
    }

    #[test]
    fn open_errors_become_connect_errors() {
        assert_eq!(
            map_open_error(failure(ffi::SQLITE_ERROR)).code,
            DbErrorCode::Connect
        );
        assert_eq!(
            map_open_error(failure(ffi::SQLITE_PERM)).code,
            DbErrorCode::Permission
        );
    }
}
