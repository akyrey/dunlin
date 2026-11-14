//! Bound parameters. Every value is sent as text and cast in the statement
//! (`$1::text::integer`, see `PostgresDialect::placeholder`), so one rule covers every type.

use dbc_core::{DbError, DbErrorCode, Value};

/// The text PostgreSQL parses for `value`; `None` for `NULL`.
pub(crate) fn as_text(value: &Value) -> Result<Option<String>, DbError> {
    Ok(match value {
        Value::Null => None,
        Value::Bool(b) => Some(b.to_string()),
        Value::Int(i) => Some(i.to_string()),
        Value::UInt(u) => Some(u.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Bytes(bytes) => Some(format!(
            "\\x{}",
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        )),
        Value::Decimal(s)
        | Value::Text(s)
        | Value::Json(s)
        | Value::Xml(s)
        | Value::Uuid(s)
        | Value::Date(s)
        | Value::Time(s)
        | Value::Timestamp(s)
        | Value::TimestampTz(s)
        | Value::Interval(s) => Some(s.clone()),
        Value::Unknown {
            text: Some(text), ..
        } => Some(text.clone()),
        Value::Array(_) | Value::Unknown { text: None, .. } => {
            return Err(DbError::new(
                DbErrorCode::Unsupported,
                "Arrays and values without a text form cannot be used as parameters yet",
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn values_become_postgres_text() {
        assert_eq!(as_text(&Value::Null).unwrap(), None);
        assert_eq!(
            as_text(&Value::Bool(true)).unwrap().as_deref(),
            Some("true")
        );
        assert_eq!(
            as_text(&Value::UInt(u64::MAX)).unwrap().as_deref(),
            Some("18446744073709551615")
        );
        assert_eq!(
            as_text(&Value::Bytes(vec![0xde, 0x01])).unwrap().as_deref(),
            Some("\\xde01")
        );
        assert_eq!(
            as_text(&Value::Decimal("1.50".into())).unwrap().as_deref(),
            Some("1.50")
        );
        assert_eq!(
            as_text(&Value::Unknown {
                type_name: "hstore".into(),
                text: Some("a=>1".into()),
                hex: None
            })
            .unwrap()
            .as_deref(),
            Some("a=>1")
        );
        assert_eq!(
            as_text(&Value::Array(vec![])).unwrap_err().code,
            DbErrorCode::Unsupported
        );
    }
}
