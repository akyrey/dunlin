//! Built-in scalar types, plus the `citext` and `hstore` extensions.

use std::net::{Ipv4Addr, Ipv6Addr};

use dbc_core::Value;
use tokio_postgres::types::Type;

use super::reader::Reader;
use super::{hex, numeric, temporal};

const AF_INET: u8 = 2;
const AF_INET6: u8 = 3;
const IPV4_BITS: u8 = 32;
const IPV6_BITS: u8 = 128;
const JSONB_VERSION: u8 = 1;

pub(super) fn decode(ty: &Type, bytes: &[u8]) -> Option<Value> {
    let mut r = Reader::new(bytes);
    let value = match ty.name() {
        "bool" => Value::Bool(r.u8()? != 0),
        "int2" => Value::Int(r.i16()?.into()),
        "int4" => Value::Int(r.i32()?.into()),
        "int8" => Value::Int(r.i64()?),
        "oid" | "xid" | "cid" | "regclass" | "regproc" | "regtype" | "regnamespace" => {
            Value::Int(r.u32()?.into())
        }
        "float4" => Value::Float(r.array_f32()?.into()),
        "float8" => Value::Float(f64::from_bits(r.u64()?)),
        "numeric" => Value::Decimal(numeric::decode(&mut r)?),
        "money" => Value::Decimal(money(r.i64()?)),
        "text" | "varchar" | "bpchar" | "name" | "char" | "unknown" | "citext" => {
            Value::Text(text(r.rest())?)
        }
        "json" => Value::Json(text(r.rest())?),
        "jsonb" => (r.u8()? == JSONB_VERSION).then(|| text(r.rest()).map(Value::Json))??,
        "xml" => Value::Xml(text(r.rest())?),
        "uuid" => Value::Uuid(uuid(r.take(16)?)),
        "bytea" => Value::Bytes(r.rest().to_vec()),
        "date" | "time" | "timetz" | "timestamp" | "timestamptz" | "interval" => {
            temporal::decode(ty.name(), &mut r)?
        }
        name => return described(name, &mut r),
    };
    r.finish(value)
}

/// Types without a `Value` variant that we render as PostgreSQL text.
fn described(name: &str, r: &mut Reader<'_>) -> Option<Value> {
    let text = match name {
        "inet" | "cidr" => inet(r)?,
        "macaddr" => mac(r.take(6)?),
        "macaddr8" => mac(r.take(8)?),
        "bit" | "varbit" => bits(r)?,
        "pg_lsn" => {
            let lsn = r.u64()?;
            format!("{:X}/{:X}", lsn >> 32, lsn & 0xffff_ffff)
        }
        "hstore" => hstore(r)?,
        _ => return None,
    };
    let value = Value::Unknown {
        type_name: name.to_owned(),
        text: Some(text),
        hex: None,
    };
    r.finish(value)
}

pub(super) fn text(bytes: &[u8]) -> Option<String> {
    String::from_utf8(bytes.to_vec()).ok()
}

fn money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

fn uuid(b: &[u8]) -> String {
    let h = hex(b);
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

fn inet(r: &mut Reader<'_>) -> Option<String> {
    let family = r.u8()?;
    let bits = r.u8()?;
    let is_cidr = r.u8()? != 0;
    let length = r.u8()?;
    let address = r.take(length.into())?;
    let (text, full) = match family {
        AF_INET => (
            Ipv4Addr::from(<[u8; 4]>::try_from(address).ok()?).to_string(),
            IPV4_BITS,
        ),
        AF_INET6 => (
            Ipv6Addr::from(<[u8; 16]>::try_from(address).ok()?).to_string(),
            IPV6_BITS,
        ),
        _ => return None,
    };
    Some(if is_cidr || bits != full {
        format!("{text}/{bits}")
    } else {
        text
    })
}

fn mac(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn bits(r: &mut Reader<'_>) -> Option<String> {
    let length = usize::try_from(r.i32()?).ok()?;
    let bytes = r.rest();
    let text: String = (0..length)
        .map(|i| {
            let byte = bytes.get(i / 8).copied().unwrap_or(0);
            if byte & (0x80 >> (i % 8)) != 0 {
                '1'
            } else {
                '0'
            }
        })
        .collect();
    (bytes.len() == length.div_ceil(8)).then_some(text)
}

fn hstore(r: &mut Reader<'_>) -> Option<String> {
    let count = r.i32()?;
    let mut pairs = Vec::new();
    for _ in 0..count {
        let key = string(r)?.unwrap_or_default();
        let value = string(r)?.map_or_else(|| "NULL".to_owned(), |v| quote(&v));
        pairs.push(format!("{}=>{value}", quote(&key)));
    }
    Some(pairs.join(", "))
}

/// A length-prefixed string; `Some(None)` is NULL.
fn string(r: &mut Reader<'_>) -> Option<Option<String>> {
    let length = r.i32()?;
    if length < 0 {
        return Some(None);
    }
    text(r.take(usize::try_from(length).ok()?)?).map(Some)
}

fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

impl Reader<'_> {
    fn array_f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }
}
