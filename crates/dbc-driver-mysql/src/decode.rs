//! Decodes text-protocol values by column type, `UNSIGNED` flag and character set.

use dbc_core::Value;
use dbc_core::result::{ColumnMeta, TypeCategory};
use mysql_async::consts::{ColumnFlags, ColumnType};
use mysql_async::{Column, Value as MyValue};

/// Character set number MySQL uses for binary strings.
const BINARY_CHARSET: u16 = 63;

pub(crate) fn decode(column: &Column, value: Option<&MyValue>) -> Value {
    match value {
        None | Some(MyValue::NULL) => Value::Null,
        Some(MyValue::Bytes(bytes)) => decode_text(column, bytes),
        // The binary protocol (statements with parameters) sends numbers and dates natively.
        Some(other) => decode_binary(column, other).unwrap_or_else(|| Value::Unknown {
            type_name: type_name(column),
            text: Some(other.as_sql(true)),
            hex: None,
        }),
    }
}

fn decode_binary(column: &Column, value: &MyValue) -> Option<Value> {
    match *value {
        MyValue::Int(i) => Some(Value::Int(i)),
        MyValue::UInt(u) => Some(Value::UInt(u)),
        MyValue::Float(f) => Some(Value::Float(f64::from(f))),
        MyValue::Double(f) => Some(Value::Float(f)),
        MyValue::Date(year, month, day, hour, minute, second, micros) => {
            let date = format!("{year:04}-{month:02}-{day:02}");
            if matches!(
                column.column_type(),
                ColumnType::MYSQL_TYPE_DATE | ColumnType::MYSQL_TYPE_NEWDATE
            ) {
                return Some(Value::Date(date));
            }
            let fraction = fraction(column, micros);
            Some(Value::Timestamp(format!(
                "{date}T{hour:02}:{minute:02}:{second:02}{fraction}"
            )))
        }
        MyValue::Time(negative, days, hours, minutes, seconds, micros) => {
            let sign = if negative { "-" } else { "" };
            let hours = u64::from(days) * 24 + u64::from(hours);
            let fraction = fraction(column, micros);
            Some(Value::Time(format!(
                "{sign}{hours:02}:{minutes:02}:{seconds:02}{fraction}"
            )))
        }
        _ => None,
    }
}

/// `.123` for a column with 3 fractional digits, as the text protocol shows it.
fn fraction(column: &Column, micros: u32) -> String {
    let digits = usize::from(column.decimals().min(6));
    if digits == 0 {
        return String::new();
    }
    format!(".{:06}", micros)[..=digits].to_owned()
}

fn decode_text(column: &Column, bytes: &[u8]) -> Value {
    let text = || String::from_utf8(bytes.to_vec());
    let parsed = match column.column_type() {
        ColumnType::MYSQL_TYPE_TINY
        | ColumnType::MYSQL_TYPE_SHORT
        | ColumnType::MYSQL_TYPE_INT24
        | ColumnType::MYSQL_TYPE_LONG
        | ColumnType::MYSQL_TYPE_YEAR => text().ok().and_then(|t| t.parse().ok()).map(Value::Int),
        ColumnType::MYSQL_TYPE_LONGLONG if is_unsigned(column) => {
            text().ok().and_then(|t| t.parse().ok()).map(Value::UInt)
        }
        ColumnType::MYSQL_TYPE_LONGLONG => text().ok().and_then(|t| t.parse().ok()).map(Value::Int),
        ColumnType::MYSQL_TYPE_FLOAT | ColumnType::MYSQL_TYPE_DOUBLE => {
            text().ok().and_then(|t| t.parse().ok()).map(Value::Float)
        }
        ColumnType::MYSQL_TYPE_DECIMAL | ColumnType::MYSQL_TYPE_NEWDECIMAL => {
            text().ok().map(Value::Decimal)
        }
        ColumnType::MYSQL_TYPE_DATE | ColumnType::MYSQL_TYPE_NEWDATE => {
            text().ok().map(Value::Date)
        }
        ColumnType::MYSQL_TYPE_TIME | ColumnType::MYSQL_TYPE_TIME2 => text().ok().map(Value::Time),
        ColumnType::MYSQL_TYPE_DATETIME
        | ColumnType::MYSQL_TYPE_DATETIME2
        | ColumnType::MYSQL_TYPE_TIMESTAMP
        | ColumnType::MYSQL_TYPE_TIMESTAMP2 => text().ok().map(|t| Value::Timestamp(iso(&t))),
        ColumnType::MYSQL_TYPE_JSON => text().ok().map(Value::Json),
        ColumnType::MYSQL_TYPE_BIT => Some(bits(column, bytes)),
        ColumnType::MYSQL_TYPE_GEOMETRY => None,
        _ if is_binary(column) => Some(Value::Bytes(bytes.to_vec())),
        _ => Some(text().map_or_else(|_| Value::Bytes(bytes.to_vec()), Value::Text)),
    };
    parsed.unwrap_or_else(|| Value::Unknown {
        type_name: type_name(column),
        text: None,
        hex: Some(bytes.iter().map(|b| format!("{b:02x}")).collect()),
    })
}

/// `2026-11-14 10:00:00` → `2026-11-14T10:00:00`, matching the other drivers.
fn iso(text: &str) -> String {
    text.replacen(' ', "T", 1)
}

/// `BIT(n)` arrives as big-endian bytes; shown as `n` binary digits.
fn bits(column: &Column, bytes: &[u8]) -> Value {
    let all: String = bytes.iter().map(|b| format!("{b:08b}")).collect();
    let width = usize::try_from(column.column_length())
        .unwrap_or(all.len())
        .min(all.len());
    let text = all[all.len() - width..].to_owned();
    Value::Unknown {
        type_name: type_name(column),
        text: Some(text),
        hex: None,
    }
}

fn is_unsigned(column: &Column) -> bool {
    column.flags().contains(ColumnFlags::UNSIGNED_FLAG)
}

fn is_binary(column: &Column) -> bool {
    column.character_set() == BINARY_CHARSET
        && matches!(
            column.column_type(),
            ColumnType::MYSQL_TYPE_STRING
                | ColumnType::MYSQL_TYPE_VAR_STRING
                | ColumnType::MYSQL_TYPE_VARCHAR
                | ColumnType::MYSQL_TYPE_BLOB
                | ColumnType::MYSQL_TYPE_TINY_BLOB
                | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
                | ColumnType::MYSQL_TYPE_LONG_BLOB
        )
}

pub(crate) fn column_meta(column: &Column) -> ColumnMeta {
    ColumnMeta {
        name: column.name_str().into_owned(),
        type_name: type_name(column),
        category: category(column),
        nullable: Some(!column.flags().contains(ColumnFlags::NOT_NULL_FLAG)),
    }
}

fn category(column: &Column) -> TypeCategory {
    match column.column_type() {
        ColumnType::MYSQL_TYPE_TINY
        | ColumnType::MYSQL_TYPE_SHORT
        | ColumnType::MYSQL_TYPE_INT24
        | ColumnType::MYSQL_TYPE_LONG
        | ColumnType::MYSQL_TYPE_LONGLONG
        | ColumnType::MYSQL_TYPE_YEAR => TypeCategory::Integer,
        ColumnType::MYSQL_TYPE_FLOAT | ColumnType::MYSQL_TYPE_DOUBLE => TypeCategory::Float,
        ColumnType::MYSQL_TYPE_DECIMAL | ColumnType::MYSQL_TYPE_NEWDECIMAL => TypeCategory::Decimal,
        ColumnType::MYSQL_TYPE_DATE
        | ColumnType::MYSQL_TYPE_NEWDATE
        | ColumnType::MYSQL_TYPE_TIME
        | ColumnType::MYSQL_TYPE_TIME2
        | ColumnType::MYSQL_TYPE_DATETIME
        | ColumnType::MYSQL_TYPE_DATETIME2
        | ColumnType::MYSQL_TYPE_TIMESTAMP
        | ColumnType::MYSQL_TYPE_TIMESTAMP2 => TypeCategory::Temporal,
        ColumnType::MYSQL_TYPE_JSON => TypeCategory::Json,
        ColumnType::MYSQL_TYPE_BIT | ColumnType::MYSQL_TYPE_GEOMETRY => TypeCategory::Other,
        _ if is_binary(column) => TypeCategory::Binary,
        _ => TypeCategory::Text,
    }
}

/// A readable type name such as `BIGINT UNSIGNED`, `VARBINARY` or `ENUM`.
fn type_name(column: &Column) -> String {
    let flags = column.flags();
    if flags.contains(ColumnFlags::ENUM_FLAG) {
        return "ENUM".to_owned();
    }
    if flags.contains(ColumnFlags::SET_FLAG) {
        return "SET".to_owned();
    }
    let binary = is_binary(column);
    let base = match column.column_type() {
        ColumnType::MYSQL_TYPE_TINY => "TINYINT",
        ColumnType::MYSQL_TYPE_SHORT => "SMALLINT",
        ColumnType::MYSQL_TYPE_INT24 => "MEDIUMINT",
        ColumnType::MYSQL_TYPE_LONG => "INT",
        ColumnType::MYSQL_TYPE_LONGLONG => "BIGINT",
        ColumnType::MYSQL_TYPE_YEAR => "YEAR",
        ColumnType::MYSQL_TYPE_FLOAT => "FLOAT",
        ColumnType::MYSQL_TYPE_DOUBLE => "DOUBLE",
        ColumnType::MYSQL_TYPE_DECIMAL | ColumnType::MYSQL_TYPE_NEWDECIMAL => "DECIMAL",
        ColumnType::MYSQL_TYPE_DATE | ColumnType::MYSQL_TYPE_NEWDATE => "DATE",
        ColumnType::MYSQL_TYPE_TIME | ColumnType::MYSQL_TYPE_TIME2 => "TIME",
        ColumnType::MYSQL_TYPE_DATETIME | ColumnType::MYSQL_TYPE_DATETIME2 => "DATETIME",
        ColumnType::MYSQL_TYPE_TIMESTAMP | ColumnType::MYSQL_TYPE_TIMESTAMP2 => "TIMESTAMP",
        ColumnType::MYSQL_TYPE_JSON => "JSON",
        ColumnType::MYSQL_TYPE_BIT => "BIT",
        ColumnType::MYSQL_TYPE_GEOMETRY => "GEOMETRY",
        ColumnType::MYSQL_TYPE_STRING if binary => "BINARY",
        ColumnType::MYSQL_TYPE_STRING => "CHAR",
        ColumnType::MYSQL_TYPE_VAR_STRING | ColumnType::MYSQL_TYPE_VARCHAR if binary => "VARBINARY",
        ColumnType::MYSQL_TYPE_VAR_STRING | ColumnType::MYSQL_TYPE_VARCHAR => "VARCHAR",
        _ if binary => "BLOB",
        ColumnType::MYSQL_TYPE_BLOB
        | ColumnType::MYSQL_TYPE_TINY_BLOB
        | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
        | ColumnType::MYSQL_TYPE_LONG_BLOB => "TEXT",
        ColumnType::MYSQL_TYPE_NULL => "NULL",
        _ => "UNKNOWN",
    };
    let numeric =
        category(column) == TypeCategory::Integer || category(column) == TypeCategory::Decimal;
    if numeric && flags.contains(ColumnFlags::UNSIGNED_FLAG) {
        format!("{base} UNSIGNED")
    } else {
        base.to_owned()
    }
}
