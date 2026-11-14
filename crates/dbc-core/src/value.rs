//! Database values and their wire representation (see `docs/DRIVER_API.md#value-model`).

mod wire;

#[cfg(test)]
mod tests;

pub use wire::{BytesPayload, TextPayload, WireError, WireValue};

/// Values larger than this (in bytes) are truncated when prepared for display.
pub const DEFAULT_DISPLAY_LIMIT: usize = 64 * 1024;

/// A single database value, decoded by a driver.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    /// MySQL `BIGINT UNSIGNED`.
    UInt(u64),
    Float(f64),
    /// Exact text from the server.
    Decimal(String),
    Text(String),
    Bytes(Vec<u8>),
    /// Original text, not re-serialised.
    Json(String),
    Xml(String),
    Uuid(String),
    /// ISO 8601.
    Date(String),
    Time(String),
    /// Without zone.
    Timestamp(String),
    /// With offset.
    TimestampTz(String),
    Interval(String),
    Array(Vec<Value>),
    Unknown {
        type_name: String,
        text: Option<String>,
        hex: Option<String>,
    },
}

impl Value {
    /// Converts to the wire form, truncating long text and binary payloads to `limit` bytes.
    pub fn into_wire(self, limit: usize) -> WireValue {
        wire::to_wire(self, limit)
    }
}

impl serde::Serialize for Value {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.clone().into_wire(usize::MAX).serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Value {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireValue::deserialize(deserializer)?;
        Value::try_from(wire).map_err(serde::de::Error::custom)
    }
}
