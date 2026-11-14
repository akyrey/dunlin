//! `AppError`: the only error shape the frontend sees (docs/IPC.md#error-shape).

use dbc_core::DbError;
use dbc_core::sql::{Danger, byte_to_utf16};
use dbc_store::StoreError;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS, thiserror::Error)]
#[error("{code}: {message}")]
pub struct AppError {
    /// Stable `snake_case` code.
    pub code: String,
    /// Safe to show to the user.
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "unknown", optional)]
    pub detail: Option<serde_json::Value>,
}

impl AppError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(self, detail: serde_json::Value) -> Self {
        Self {
            detail: Some(detail),
            ..self
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_argument", message)
    }

    pub fn not_found(what: &str) -> Self {
        Self::new("not_found", format!("{what} was not found"))
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message)
    }

    pub fn read_only() -> Self {
        Self::new(
            "read_only",
            "This connection is read-only; the statement would modify data",
        )
    }

    pub fn confirmation_required(danger: Danger) -> Self {
        Self::new("confirmation_required", danger.reason())
            .with_detail(json!({ "reason": danger.reason() }))
    }

    /// A driver error for `statement`, with the byte position converted to UTF-16 offsets.
    pub fn from_db(error: DbError, statement: &str) -> Self {
        let position = error.position.map(|p| byte_to_utf16(statement, p));
        Self::db(error, position)
    }

    fn db(error: DbError, position: Option<usize>) -> Self {
        let detail = json!({
            "position": position,
            "sqlstate": error.sqlstate,
            "serverDetail": error.detail,
        });
        let has_detail = detail
            .as_object()
            .is_some_and(|d| d.values().any(|v| !v.is_null()));
        let base = Self::new(error.code.as_str(), error.message);
        if has_detail {
            base.with_detail(strip_nulls(detail))
        } else {
            base
        }
    }
}

fn strip_nulls(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            serde_json::Value::Object(map.into_iter().filter(|(_, v)| !v.is_null()).collect())
        }
        other => other,
    }
}

impl From<DbError> for AppError {
    fn from(error: DbError) -> Self {
        Self::db(error, None)
    }
}

impl From<StoreError> for AppError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::NotFound(_) => Self::not_found("The connection"),
            StoreError::Invalid(message) => Self::invalid(message),
            StoreError::TooNew { .. } => Self::new("store_too_new", error.to_string()),
            StoreError::Database(_) | StoreError::Json(_) => {
                Self::internal(format!("Local storage error: {error}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use dbc_core::DbErrorCode;

    use super::*;

    #[test]
    fn driver_errors_keep_code_and_message() {
        let error = AppError::from(DbError::new(DbErrorCode::Syntax, "near FRM"));

        assert_eq!(
            (error.code.as_str(), error.message.as_str()),
            ("syntax", "near FRM")
        );
        assert_eq!(error.detail, None);
    }

    #[test]
    fn positions_are_converted_to_utf16_offsets() {
        let statement = "SELECT '🌍' FRM t";
        let byte_position = statement.find("FRM").unwrap();
        let error = DbError::new(DbErrorCode::Syntax, "x").with_position(byte_position);

        let app = AppError::from_db(error, statement);

        assert_eq!(app.detail, Some(json!({ "position": 12 })));
    }

    #[test]
    fn server_detail_and_sqlstate_are_passed_on() {
        let error = DbError::new(DbErrorCode::Constraint, "dup")
            .with_detail("Key (id)=(1) exists")
            .with_sqlstate("23505");

        let app = AppError::from(error);

        assert_eq!(
            app.detail,
            Some(json!({ "sqlstate": "23505", "serverDetail": "Key (id)=(1) exists" }))
        );
    }

    #[test]
    fn store_errors_map_to_stable_codes() {
        let not_found = AppError::from(StoreError::NotFound("x".into()));
        let invalid = AppError::from(StoreError::Invalid("bad".into()));
        let too_new = AppError::from(StoreError::TooNew {
            found: 9,
            supported: 1,
        });

        assert_eq!(not_found.code, "not_found");
        assert_eq!(
            (invalid.code.as_str(), invalid.message.as_str()),
            ("invalid_argument", "bad")
        );
        assert_eq!(too_new.code, "store_too_new");
    }

    #[test]
    fn confirmation_carries_the_reason() {
        let error = AppError::confirmation_required(Danger::Drop);

        assert_eq!(error.code, "confirmation_required");
        assert_eq!(
            error.detail,
            Some(json!({ "reason": Danger::Drop.reason() }))
        );
    }

    #[test]
    fn serialises_without_empty_detail() {
        let json = serde_json::to_value(AppError::not_found("Session")).unwrap();

        assert_eq!(
            json,
            json!({ "code": "not_found", "message": "Session was not found" })
        );
    }
}
