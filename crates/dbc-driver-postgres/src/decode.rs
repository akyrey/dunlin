//! Decodes PostgreSQL binary-format values into [`Value`] (DECISIONS D-016).
//!
//! Types with a dedicated `Value` variant decode to it. Other types we understand (ranges,
//! composites, network types, `hstore`, ...) become `Value::Unknown` with PostgreSQL-style text.
//! Anything else, or any malformed payload, becomes `Value::Unknown` with a hex dump.

mod composite;
mod numeric;
mod reader;
mod scalar;
mod temporal;

#[cfg(test)]
mod tests;

use dbc_core::Value;
use dbc_core::result::TypeCategory;
use tokio_postgres::types::{Kind, Type};

use reader::Reader;

/// Decodes one cell. `raw` is `None` for SQL `NULL`.
pub(crate) fn decode(ty: &Type, raw: Option<&[u8]>) -> Value {
    match raw {
        None => Value::Null,
        Some(bytes) => decode_bytes(ty, bytes).unwrap_or_else(|| unknown_hex(ty, bytes)),
    }
}

fn decode_bytes(ty: &Type, bytes: &[u8]) -> Option<Value> {
    match ty.kind() {
        Kind::Domain(base) => decode_bytes(base, bytes),
        Kind::Enum(_) => scalar::text(bytes).map(Value::Text),
        Kind::Array(element) => composite::array(element, bytes),
        Kind::Range(element) => composite::range(element, bytes).map(|t| described(ty, t)),
        Kind::Multirange(element) => {
            composite::multirange(element, bytes).map(|t| described(ty, t))
        }
        Kind::Composite(fields) => {
            let types: Vec<Type> = fields.iter().map(|f| f.type_().clone()).collect();
            composite::record(Some(&types), bytes).map(|t| described(ty, t))
        }
        Kind::Pseudo if ty.name() == "record" => {
            composite::record(None, bytes).map(|t| described(ty, t))
        }
        _ => scalar::decode(ty, bytes),
    }
}

/// A value we can render as text but that has no dedicated `Value` variant.
fn described(ty: &Type, text: String) -> Value {
    Value::Unknown {
        type_name: ty.name().to_owned(),
        text: Some(text),
        hex: None,
    }
}

fn unknown_hex(ty: &Type, bytes: &[u8]) -> Value {
    Value::Unknown {
        type_name: ty.name().to_owned(),
        text: None,
        hex: Some(hex(bytes)),
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// PostgreSQL text form of a decoded value, for nesting inside ranges and records.
fn value_text(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::Bool(b) => if *b { "t" } else { "f" }.to_owned(),
        Value::Int(i) => i.to_string(),
        Value::UInt(u) => u.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bytes(b) => format!("\\x{}", hex(b)),
        Value::Array(items) => {
            format!(
                "{{{}}}",
                items.iter().map(value_text).collect::<Vec<_>>().join(",")
            )
        }
        Value::Unknown { text, hex, .. } => text
            .clone()
            .or_else(|| hex.as_ref().map(|h| format!("\\x{h}")))
            .unwrap_or_default(),
        Value::Decimal(s)
        | Value::Text(s)
        | Value::Json(s)
        | Value::Xml(s)
        | Value::Uuid(s)
        | Value::Date(s)
        | Value::Time(s)
        | Value::Timestamp(s)
        | Value::TimestampTz(s)
        | Value::Interval(s) => s.clone(),
    }
}

/// The category the frontend uses for alignment and viewers.
pub(crate) fn category(ty: &Type) -> TypeCategory {
    match ty.kind() {
        Kind::Domain(base) => return category(base),
        Kind::Enum(_) => return TypeCategory::Text,
        Kind::Array(_) => return TypeCategory::Array,
        Kind::Range(_) | Kind::Multirange(_) | Kind::Composite(_) | Kind::Pseudo => {
            return TypeCategory::Other;
        }
        Kind::Simple => {}
        _ => return TypeCategory::Other,
    }
    match ty.name() {
        "bool" => TypeCategory::Bool,
        "int2" | "int4" | "int8" | "oid" | "xid" | "cid" => TypeCategory::Integer,
        "float4" | "float8" => TypeCategory::Float,
        "numeric" | "money" => TypeCategory::Decimal,
        "text" | "varchar" | "bpchar" | "name" | "char" | "citext" | "unknown" => {
            TypeCategory::Text
        }
        "json" | "jsonb" => TypeCategory::Json,
        "xml" => TypeCategory::Xml,
        "bytea" => TypeCategory::Binary,
        "date" | "time" | "timetz" | "timestamp" | "timestamptz" | "interval" => {
            TypeCategory::Temporal
        }
        "uuid" => TypeCategory::Uuid,
        _ => TypeCategory::Other,
    }
}

/// Reads a length-prefixed element (`-1` means NULL) and decodes it as `ty`.
fn element(ty: &Type, reader: &mut Reader<'_>) -> Option<Value> {
    let length = reader.i32()?;
    if length < 0 {
        return Some(Value::Null);
    }
    let bytes = reader.take(usize::try_from(length).ok()?)?;
    Some(decode(ty, Some(bytes)))
}
