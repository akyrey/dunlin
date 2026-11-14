//! Maps `mysql_async` errors to [`DbError`] by server error number and SQLSTATE.

use dbc_core::{DbError, DbErrorCode};
use mysql_async::{DriverError, Error, IoError};

pub(crate) fn map_error(error: &Error) -> DbError {
    match error {
        Error::Server(server) => {
            DbError::new(code_for(server.code, &server.state), server.message.clone())
                .with_sqlstate(server.state.clone())
        }
        Error::Io(IoError::Tls(tls)) => DbError::new(DbErrorCode::Tls, format!("TLS error: {tls}")),
        Error::Io(IoError::Io(io)) if io.kind() == std::io::ErrorKind::TimedOut => {
            DbError::new(DbErrorCode::Timeout, "The server did not respond in time")
        }
        Error::Io(IoError::Io(io)) if tls_cause(io).is_some() => {
            let cause = tls_cause(io).map(ToString::to_string).unwrap_or_default();
            DbError::new(DbErrorCode::Tls, format!("TLS error: {cause}"))
        }
        Error::Io(io) => DbError::new(
            DbErrorCode::ConnectionLost,
            format!("Connection error: {io}"),
        ),
        Error::Driver(DriverError::ConnectionClosed) => DbError::new(
            DbErrorCode::ConnectionLost,
            "The connection to the server was lost",
        ),
        Error::Driver(DriverError::NoClientSslFlagFromServer) => {
            DbError::new(DbErrorCode::Tls, "The server does not support TLS")
        }
        Error::Driver(driver) => DbError::new(DbErrorCode::Internal, driver.to_string()),
        other => DbError::new(DbErrorCode::Internal, other.to_string()),
    }
}

/// The rustls error inside an I/O error, if the failure was a TLS handshake.
fn tls_cause(io: &std::io::Error) -> Option<&rustls::Error> {
    let mut cause: Option<&(dyn std::error::Error + 'static)> = io.get_ref().map(|e| e as _);
    while let Some(error) = cause {
        if let Some(tls) = error.downcast_ref::<rustls::Error>() {
            return Some(tls);
        }
        cause = error.source();
    }
    None
}

/// While connecting, I/O failures mean the server could not be reached.
pub(crate) fn map_connect_error(error: &Error) -> DbError {
    let mapped = map_error(error);
    match mapped.code {
        DbErrorCode::ConnectionLost
        | DbErrorCode::Syntax
        | DbErrorCode::Execution
        | DbErrorCode::Internal => DbError {
            code: DbErrorCode::Connect,
            ..mapped
        },
        _ => mapped,
    }
}

pub(crate) fn code_for(number: u16, sqlstate: &str) -> DbErrorCode {
    match number {
        1045 | 1698 => DbErrorCode::Auth,
        1044 | 1142 | 1143 | 1227 | 1370 => DbErrorCode::Permission,
        1049 => DbErrorCode::Connect,
        1317 => DbErrorCode::Cancelled,
        1792 | 1290 => DbErrorCode::ReadOnly,
        3024 | 1969 => DbErrorCode::Timeout,
        2006 | 2013 | 1053 => DbErrorCode::ConnectionLost,
        1062 | 1451 | 1452 | 1048 | 3819 | 4025 => DbErrorCode::Constraint,
        1064 | 1146 | 1054 | 1149 => DbErrorCode::Syntax,
        _ if sqlstate.starts_with("23") => DbErrorCode::Constraint,
        _ if sqlstate.starts_with("42") => DbErrorCode::Syntax,
        _ => DbErrorCode::Execution,
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(1045, "28000", DbErrorCode::Auth)]
    #[case(1044, "42000", DbErrorCode::Permission)]
    #[case(1049, "42000", DbErrorCode::Connect)]
    #[case(1317, "70100", DbErrorCode::Cancelled)]
    #[case(1792, "25006", DbErrorCode::ReadOnly)]
    #[case(3024, "HY000", DbErrorCode::Timeout)]
    #[case(1062, "23000", DbErrorCode::Constraint)]
    #[case(4025, "23000", DbErrorCode::Constraint)]
    #[case(1064, "42000", DbErrorCode::Syntax)]
    #[case(1146, "42S02", DbErrorCode::Syntax)]
    #[case(9999, "23000", DbErrorCode::Constraint)]
    #[case(1365, "22012", DbErrorCode::Execution)]
    fn maps_server_errors(
        #[case] number: u16,
        #[case] sqlstate: &str,
        #[case] expected: DbErrorCode,
    ) {
        assert_eq!(code_for(number, sqlstate), expected);
    }
}
