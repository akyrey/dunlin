//! The common driver error.

use serde::Serialize;

/// Stable classification of a [`DbError`]. Serialised as `snake_case`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DbErrorCode {
    Connect,
    Auth,
    Tls,
    Timeout,
    Cancelled,
    Syntax,
    Constraint,
    Permission,
    /// A write was attempted on a read-only connection.
    ReadOnly,
    ConnectionLost,
    Unsupported,
    RowCountMismatch,
    Internal,
}

impl DbErrorCode {
    /// The stable `snake_case` code shown to the frontend.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Auth => "auth",
            Self::Tls => "tls",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::Syntax => "syntax",
            Self::Constraint => "constraint",
            Self::Permission => "permission",
            Self::ReadOnly => "read_only",
            Self::ConnectionLost => "connection_lost",
            Self::Unsupported => "unsupported",
            Self::RowCountMismatch => "row_count_mismatch",
            Self::Internal => "internal",
        }
    }
}

/// An error reported by a driver. `message` must be safe to show to the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct DbError {
    pub code: DbErrorCode,
    pub message: String,
    /// Server detail or hint.
    pub detail: Option<String>,
    /// Byte offset into the statement, when the server reports one.
    pub position: Option<usize>,
    pub sqlstate: Option<String>,
}

impl DbError {
    pub fn new(code: DbErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
            position: None,
            sqlstate: None,
        }
    }

    /// The operation is not implemented by this driver (yet).
    pub fn unsupported(operation: &str) -> Self {
        Self::new(
            DbErrorCode::Unsupported,
            format!("{operation} is not supported by this driver"),
        )
    }

    pub fn with_detail(self, detail: impl Into<String>) -> Self {
        Self {
            detail: Some(detail.into()),
            ..self
        }
    }

    pub fn with_position(self, position: usize) -> Self {
        Self {
            position: Some(position),
            ..self
        }
    }

    pub fn with_sqlstate(self, sqlstate: impl Into<String>) -> Self {
        Self {
            sqlstate: Some(sqlstate.into()),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_methods_set_optional_fields() {
        let error = DbError::new(DbErrorCode::Syntax, "near FROM")
            .with_detail("hint")
            .with_position(7)
            .with_sqlstate("42601");

        assert_eq!(error.detail.as_deref(), Some("hint"));
        assert_eq!(error.position, Some(7));
        assert_eq!(error.sqlstate.as_deref(), Some("42601"));
        assert_eq!(error.to_string(), "near FROM");
    }

    #[test]
    fn codes_serialise_like_as_str() {
        let all = [
            DbErrorCode::Connect,
            DbErrorCode::Auth,
            DbErrorCode::Tls,
            DbErrorCode::Timeout,
            DbErrorCode::Cancelled,
            DbErrorCode::Syntax,
            DbErrorCode::Constraint,
            DbErrorCode::Permission,
            DbErrorCode::ReadOnly,
            DbErrorCode::ConnectionLost,
            DbErrorCode::Unsupported,
            DbErrorCode::RowCountMismatch,
            DbErrorCode::Internal,
        ];

        for code in all {
            let json = serde_json::to_value(code).unwrap();
            assert_eq!(json, serde_json::Value::String(code.as_str().into()));
        }
    }

    #[test]
    fn unsupported_names_the_operation() {
        let error = DbError::unsupported("describe_table");

        assert_eq!(error.code, DbErrorCode::Unsupported);
        assert!(error.message.contains("describe_table"));
    }
}
