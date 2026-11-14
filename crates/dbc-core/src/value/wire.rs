//! The tagged, string-payload wire format. This is the type that crosses IPC.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

use super::Value;

/// Wire form of [`Value`]: `{ "t": <tag>, "v": <string payload> }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "Value"))]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum WireValue {
    Null,
    Bool { v: bool },
    Int { v: String },
    Uint { v: String },
    Float { v: String },
    Decimal { v: String },
    Text(TextPayload),
    Bytes(BytesPayload),
    Json(TextPayload),
    Xml(TextPayload),
    Uuid { v: String },
    Date { v: String },
    Time { v: String },
    Timestamp { v: String },
    Timestamptz { v: String },
    Interval { v: String },
    Array { v: Vec<WireValue> },
    Unknown(UnknownPayload),
}

/// Text payload, possibly truncated for display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TextPayload {
    pub v: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub truncated: bool,
}

/// Base64 payload; `len` is the size of the full value in bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BytesPayload {
    pub v: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub len: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub truncated: bool,
}

/// A value of a type the driver could not decode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct UnknownPayload {
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub hex: Option<String>,
}

/// Why a wire value could not be turned back into a [`Value`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("invalid {kind} payload: {payload:?}")]
    InvalidNumber { kind: &'static str, payload: String },
    #[error("invalid base64 in bytes payload")]
    InvalidBase64,
    #[error("bytes payload length {declared} does not match decoded length {actual}")]
    LengthMismatch { declared: u64, actual: u64 },
    #[error("a truncated value cannot be used as input")]
    Truncated,
}

pub(super) fn to_wire(value: Value, limit: usize) -> WireValue {
    match value {
        Value::Null => WireValue::Null,
        Value::Bool(v) => WireValue::Bool { v },
        Value::Int(v) => WireValue::Int { v: v.to_string() },
        Value::UInt(v) => WireValue::Uint { v: v.to_string() },
        Value::Float(v) => WireValue::Float { v: format_float(v) },
        Value::Decimal(v) => WireValue::Decimal { v },
        Value::Text(v) => WireValue::Text(text_payload(v, limit)),
        Value::Bytes(v) => WireValue::Bytes(bytes_payload(&v, limit)),
        Value::Json(v) => WireValue::Json(text_payload(v, limit)),
        Value::Xml(v) => WireValue::Xml(text_payload(v, limit)),
        Value::Uuid(v) => WireValue::Uuid { v },
        Value::Date(v) => WireValue::Date { v },
        Value::Time(v) => WireValue::Time { v },
        Value::Timestamp(v) => WireValue::Timestamp { v },
        Value::TimestampTz(v) => WireValue::Timestamptz { v },
        Value::Interval(v) => WireValue::Interval { v },
        Value::Array(items) => WireValue::Array {
            v: items.into_iter().map(|item| to_wire(item, limit)).collect(),
        },
        Value::Unknown {
            type_name,
            text,
            hex,
        } => WireValue::Unknown(UnknownPayload {
            type_name,
            text,
            hex,
        }),
    }
}

impl TryFrom<WireValue> for Value {
    type Error = WireError;

    fn try_from(wire: WireValue) -> Result<Self, Self::Error> {
        Ok(match wire {
            WireValue::Null => Value::Null,
            WireValue::Bool { v } => Value::Bool(v),
            WireValue::Int { v } => Value::Int(parse_number(v, "int")?),
            WireValue::Uint { v } => Value::UInt(parse_number(v, "uint")?),
            WireValue::Float { v } => Value::Float(parse_float(v)?),
            WireValue::Decimal { v } => Value::Decimal(v),
            WireValue::Text(p) => Value::Text(untruncated(p)?),
            WireValue::Bytes(p) => Value::Bytes(decode_bytes(p)?),
            WireValue::Json(p) => Value::Json(untruncated(p)?),
            WireValue::Xml(p) => Value::Xml(untruncated(p)?),
            WireValue::Uuid { v } => Value::Uuid(v),
            WireValue::Date { v } => Value::Date(v),
            WireValue::Time { v } => Value::Time(v),
            WireValue::Timestamp { v } => Value::Timestamp(v),
            WireValue::Timestamptz { v } => Value::TimestampTz(v),
            WireValue::Interval { v } => Value::Interval(v),
            WireValue::Array { v } => Value::Array(
                v.into_iter()
                    .map(Value::try_from)
                    .collect::<Result<_, _>>()?,
            ),
            WireValue::Unknown(UnknownPayload {
                type_name,
                text,
                hex,
            }) => Value::Unknown {
                type_name,
                text,
                hex,
            },
        })
    }
}

const NAN: &str = "NaN";
const INFINITY: &str = "Infinity";
const NEG_INFINITY: &str = "-Infinity";

/// Shortest representation that round-trips; non-finite values use JavaScript spelling.
fn format_float(v: f64) -> String {
    if v.is_nan() {
        NAN.to_owned()
    } else if v == f64::INFINITY {
        INFINITY.to_owned()
    } else if v == f64::NEG_INFINITY {
        NEG_INFINITY.to_owned()
    } else {
        // `Debug` uses exponent notation for very large and small magnitudes; `Display` does not.
        format!("{v:?}")
    }
}

fn parse_float(v: String) -> Result<f64, WireError> {
    match v.as_str() {
        NAN => Ok(f64::NAN),
        INFINITY => Ok(f64::INFINITY),
        NEG_INFINITY => Ok(f64::NEG_INFINITY),
        // Reject Rust-only spellings ("inf", "nan") so the format stays canonical.
        s if s
            .chars()
            .any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E') =>
        {
            Err(WireError::InvalidNumber {
                kind: "float",
                payload: v,
            })
        }
        s => s.parse().map_err(|_| WireError::InvalidNumber {
            kind: "float",
            payload: v,
        }),
    }
}

fn parse_number<T: std::str::FromStr>(v: String, kind: &'static str) -> Result<T, WireError> {
    v.parse()
        .map_err(|_| WireError::InvalidNumber { kind, payload: v })
}

fn text_payload(v: String, limit: usize) -> TextPayload {
    if v.len() <= limit {
        return TextPayload {
            v,
            truncated: false,
        };
    }
    let mut end = limit;
    while !v.is_char_boundary(end) {
        end -= 1;
    }
    TextPayload {
        v: v[..end].to_owned(),
        truncated: true,
    }
}

fn bytes_payload(bytes: &[u8], limit: usize) -> BytesPayload {
    let shown = &bytes[..bytes.len().min(limit)];
    BytesPayload {
        v: BASE64.encode(shown),
        len: bytes.len() as u64,
        truncated: shown.len() < bytes.len(),
    }
}

fn untruncated(payload: TextPayload) -> Result<String, WireError> {
    if payload.truncated {
        Err(WireError::Truncated)
    } else {
        Ok(payload.v)
    }
}

fn decode_bytes(payload: BytesPayload) -> Result<Vec<u8>, WireError> {
    if payload.truncated {
        return Err(WireError::Truncated);
    }
    let bytes = BASE64
        .decode(payload.v)
        .map_err(|_| WireError::InvalidBase64)?;
    let actual = bytes.len() as u64;
    if actual != payload.len {
        return Err(WireError::LengthMismatch {
            declared: payload.len,
            actual,
        });
    }
    Ok(bytes)
}
