//! Converts SQLite cells to [`Value`] and declared types to [`TypeCategory`].

use dbc_core::Value;
use dbc_core::result::TypeCategory;
use rusqlite::types::ValueRef;

/// The stored value decides the variant; SQLite is dynamically typed per cell.
pub(crate) fn decode(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::Int(i),
        ValueRef::Real(f) => Value::Float(f),
        ValueRef::Text(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => Value::Text(text.to_owned()),
            Err(_) => Value::Bytes(bytes.to_vec()),
        },
        ValueRef::Blob(bytes) => Value::Bytes(bytes.to_vec()),
    }
}

/// Category from a declared column type, following SQLite's affinity rules plus common names.
pub(crate) fn category_for_declared(declared: &str) -> TypeCategory {
    let t = declared.to_ascii_lowercase();
    let has = |needle: &str| t.contains(needle);
    if has("bool") {
        TypeCategory::Bool
    } else if has("json") {
        TypeCategory::Json
    } else if has("xml") {
        TypeCategory::Xml
    } else if has("uuid") || has("guid") {
        TypeCategory::Uuid
    } else if has("date") || has("time") {
        TypeCategory::Temporal
    } else if has("int") {
        TypeCategory::Integer
    } else if has("char") || has("clob") || has("text") {
        TypeCategory::Text
    } else if has("blob") || t.is_empty() {
        TypeCategory::Binary
    } else if has("real") || has("floa") || has("doub") {
        TypeCategory::Float
    } else if has("dec") || has("num") {
        TypeCategory::Decimal
    } else {
        TypeCategory::Other
    }
}

/// Category for an expression column without a declared type, from its first value.
pub(crate) fn category_for_value(value: Option<&Value>) -> TypeCategory {
    match value {
        Some(Value::Int(_)) => TypeCategory::Integer,
        Some(Value::Float(_)) => TypeCategory::Float,
        Some(Value::Text(_)) => TypeCategory::Text,
        Some(Value::Bytes(_)) => TypeCategory::Binary,
        _ => TypeCategory::Other,
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("INTEGER", TypeCategory::Integer)]
    #[case("BIGINT", TypeCategory::Integer)]
    #[case("BOOLEAN", TypeCategory::Bool)]
    #[case("VARCHAR(255)", TypeCategory::Text)]
    #[case("TEXT", TypeCategory::Text)]
    #[case("BLOB", TypeCategory::Binary)]
    #[case("", TypeCategory::Binary)]
    #[case("REAL", TypeCategory::Float)]
    #[case("DOUBLE PRECISION", TypeCategory::Float)]
    #[case("NUMERIC(12, 2)", TypeCategory::Decimal)]
    #[case("DECIMAL", TypeCategory::Decimal)]
    #[case("JSON", TypeCategory::Json)]
    #[case("XML", TypeCategory::Xml)]
    #[case("UUID", TypeCategory::Uuid)]
    #[case("DATE", TypeCategory::Temporal)]
    #[case("TIMESTAMP", TypeCategory::Temporal)]
    #[case("DATETIME", TypeCategory::Temporal)]
    #[case("GEOMETRY", TypeCategory::Other)]
    fn categorises_declared_types(#[case] declared: &str, #[case] expected: TypeCategory) {
        assert_eq!(category_for_declared(declared), expected);
    }

    #[test]
    fn invalid_utf8_text_becomes_bytes() {
        let value = decode(ValueRef::Text(&[0xff, 0xfe]));

        assert_eq!(value, Value::Bytes(vec![0xff, 0xfe]));
    }

    #[rstest]
    #[case(Some(Value::Int(1)), TypeCategory::Integer)]
    #[case(Some(Value::Float(1.0)), TypeCategory::Float)]
    #[case(Some(Value::Text(String::new())), TypeCategory::Text)]
    #[case(Some(Value::Bytes(vec![])), TypeCategory::Binary)]
    #[case(Some(Value::Null), TypeCategory::Other)]
    #[case(None, TypeCategory::Other)]
    fn categorises_expressions_by_first_value(
        #[case] value: Option<Value>,
        #[case] expected: TypeCategory,
    ) {
        assert_eq!(category_for_value(value.as_ref()), expected);
    }
}
