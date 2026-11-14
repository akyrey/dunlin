//! Container types: arrays (to `Value::Array`), and ranges, multiranges and records (to text).

use dbc_core::Value;
use tokio_postgres::types::Type;

use super::reader::Reader;
use super::{element, value_text};

const RANGE_EMPTY: u8 = 0x01;
const RANGE_LOWER_INCLUSIVE: u8 = 0x02;
const RANGE_UPPER_INCLUSIVE: u8 = 0x04;
const RANGE_LOWER_INFINITE: u8 = 0x08;
const RANGE_UPPER_INFINITE: u8 = 0x10;
/// PostgreSQL's own limit on array dimensions.
const MAX_DIMENSIONS: i32 = 6;

/// Multi-dimensional arrays become nested `Value::Array`s.
pub(super) fn array(element_type: &Type, bytes: &[u8]) -> Option<Value> {
    let mut r = Reader::new(bytes);
    let dimensions = r.i32()?;
    if !(0..=MAX_DIMENSIONS).contains(&dimensions) {
        return None;
    }
    let _has_nulls = r.i32()?;
    let _element_oid = r.u32()?;
    let mut lengths = Vec::new();
    for _ in 0..dimensions {
        lengths.push(usize::try_from(r.i32()?).ok()?);
        let _lower_bound = r.i32()?;
    }
    let total: usize = lengths.iter().product();
    let mut flat = Vec::with_capacity(total.min(bytes.len()));
    if dimensions > 0 {
        for _ in 0..total {
            flat.push(element(element_type, &mut r)?);
        }
    }
    let mut items = flat.into_iter();
    let value = nest(&lengths, &mut items);
    r.finish(value)
}

fn nest(lengths: &[usize], items: &mut impl Iterator<Item = Value>) -> Value {
    match lengths.split_first() {
        None => Value::Array(Vec::new()),
        Some((&length, [])) => Value::Array(items.take(length).collect()),
        Some((&length, inner)) => Value::Array((0..length).map(|_| nest(inner, items)).collect()),
    }
}

/// `[1,5)`, `(,10]`, `empty`.
pub(super) fn range(element_type: &Type, bytes: &[u8]) -> Option<String> {
    let mut r = Reader::new(bytes);
    let text = range_text(element_type, &mut r)?;
    r.finish(text)
}

fn range_text(element_type: &Type, r: &mut Reader<'_>) -> Option<String> {
    let flags = r.u8()?;
    if flags & RANGE_EMPTY != 0 {
        return Some("empty".to_owned());
    }
    let lower = bound(element_type, r, flags & RANGE_LOWER_INFINITE == 0)?;
    let upper = bound(element_type, r, flags & RANGE_UPPER_INFINITE == 0)?;
    let open = if flags & RANGE_LOWER_INCLUSIVE != 0 {
        '['
    } else {
        '('
    };
    let close = if flags & RANGE_UPPER_INCLUSIVE != 0 {
        ']'
    } else {
        ')'
    };
    Some(format!("{open}{lower},{upper}{close}"))
}

fn bound(element_type: &Type, r: &mut Reader<'_>, present: bool) -> Option<String> {
    if !present {
        return Some(String::new());
    }
    element(element_type, r).map(|v| quote_if_needed(&value_text(&v)))
}

/// `{[1,2),[5,6)}`.
pub(super) fn multirange(element_type: &Type, bytes: &[u8]) -> Option<String> {
    let mut r = Reader::new(bytes);
    let count = r.i32()?;
    let mut ranges = Vec::new();
    for _ in 0..count {
        let length = usize::try_from(r.i32()?).ok()?;
        ranges.push(range(element_type, r.take(length)?)?);
    }
    r.finish(format!("{{{}}}", ranges.join(",")))
}

/// `(1,abc,)`. Field types come from the composite type, or from the OIDs for anonymous rows.
pub(super) fn record(field_types: Option<&[Type]>, bytes: &[u8]) -> Option<String> {
    let mut r = Reader::new(bytes);
    let count = usize::try_from(r.i32()?).ok()?;
    let mut fields = Vec::new();
    for index in 0..count {
        let oid = r.u32()?;
        let ty = field_types
            .and_then(|types| types.get(index).cloned())
            .or_else(|| Type::from_oid(oid));
        let value = match ty {
            Some(ty) => element(&ty, &mut r)?,
            None => opaque(oid, &mut r)?,
        };
        fields.push(match value {
            Value::Null => String::new(),
            other => quote_if_needed(&value_text(&other)),
        });
    }
    r.finish(format!("({})", fields.join(",")))
}

/// A field whose type we do not know: keep its bytes as hex.
fn opaque(oid: u32, r: &mut Reader<'_>) -> Option<Value> {
    let length = r.i32()?;
    if length < 0 {
        return Some(Value::Null);
    }
    let bytes = r.take(usize::try_from(length).ok()?)?;
    Some(Value::Unknown {
        type_name: format!("oid {oid}"),
        text: None,
        hex: Some(super::hex(bytes)),
    })
}

fn quote_if_needed(text: &str) -> String {
    let special = text.is_empty()
        || text
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, ',' | '(' | ')' | '[' | ']' | '"' | '\\'));
    if special {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        text.to_owned()
    }
}
