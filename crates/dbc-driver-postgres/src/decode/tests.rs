use dbc_core::Value;
use dbc_core::result::TypeCategory;
use rstest::rstest;
use tokio_postgres::types::{Kind, Type};

use super::*;

fn text(s: &str) -> Value {
    Value::Text(s.to_owned())
}

fn described(type_name: &str, text: &str) -> Value {
    Value::Unknown {
        type_name: type_name.to_owned(),
        text: Some(text.to_owned()),
        hex: None,
    }
}

/// Binary numeric: ndigits, weight, sign, dscale, digits.
fn numeric(weight: i16, sign: u16, scale: u16, digits: &[u16]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend((digits.len() as i16).to_be_bytes());
    b.extend(weight.to_be_bytes());
    b.extend(sign.to_be_bytes());
    b.extend(scale.to_be_bytes());
    for d in digits {
        b.extend(d.to_be_bytes());
    }
    b
}

fn element_bytes(payload: Option<&[u8]>) -> Vec<u8> {
    match payload {
        None => (-1i32).to_be_bytes().to_vec(),
        Some(p) => [(p.len() as i32).to_be_bytes().as_slice(), p].concat(),
    }
}

fn int4_array(dims: &[i32], items: &[Option<i32>]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend((dims.len() as i32).to_be_bytes());
    b.extend(1i32.to_be_bytes());
    b.extend(Type::INT4.oid().to_be_bytes());
    for d in dims {
        b.extend(d.to_be_bytes());
        b.extend(1i32.to_be_bytes());
    }
    for item in items {
        let bytes = item.map(|v| v.to_be_bytes());
        b.extend(element_bytes(bytes.as_ref().map(|x| x.as_slice())));
    }
    b
}

#[rstest]
#[case::bool(Type::BOOL, vec![1], Value::Bool(true))]
#[case::int2(Type::INT2, (-2i16).to_be_bytes().to_vec(), Value::Int(-2))]
#[case::int4(Type::INT4, 7i32.to_be_bytes().to_vec(), Value::Int(7))]
#[case::int8_max(Type::INT8, i64::MAX.to_be_bytes().to_vec(), Value::Int(i64::MAX))]
#[case::oid(Type::OID, u32::MAX.to_be_bytes().to_vec(), Value::Int(i64::from(u32::MAX)))]
#[case::float4(Type::FLOAT4, 1.5f32.to_be_bytes().to_vec(), Value::Float(1.5))]
#[case::float8(Type::FLOAT8, (-0.25f64).to_be_bytes().to_vec(), Value::Float(-0.25))]
#[case::text(Type::TEXT, b"h\xc3\xa9".to_vec(), text("hé"))]
#[case::varchar(Type::VARCHAR, b"".to_vec(), text(""))]
#[case::json(Type::JSON, br#"{"a": 1}"#.to_vec(), Value::Json(r#"{"a": 1}"#.into()))]
#[case::jsonb(Type::JSONB, b"\x01{\"a\": 1}".to_vec(), Value::Json(r#"{"a": 1}"#.into()))]
#[case::xml(Type::XML, b"<a/>".to_vec(), Value::Xml("<a/>".into()))]
#[case::bytea(Type::BYTEA, vec![0xde, 0xad], Value::Bytes(vec![0xde, 0xad]))]
#[case::uuid(
    Type::UUID,
    vec![0x6f, 0x1c, 0x2d, 0x3e, 0, 0, 0x40, 0, 0x80, 0, 0, 0, 0, 0, 0, 1],
    Value::Uuid("6f1c2d3e-0000-4000-8000-000000000001".into())
)]
#[case::money(Type::MONEY, (-1234i64).to_be_bytes().to_vec(), Value::Decimal("-12.34".into()))]
#[case::date(Type::DATE, 9814i32.to_be_bytes().to_vec(), Value::Date("2026-11-14".into()))]
#[case::date_before_epoch(Type::DATE, (-1i32).to_be_bytes().to_vec(), Value::Date("1999-12-31".into()))]
#[case::date_infinity(Type::DATE, i32::MAX.to_be_bytes().to_vec(), Value::Date("infinity".into()))]
#[case::time(Type::TIME, 37_230_500_000i64.to_be_bytes().to_vec(), Value::Time("10:20:30.5".into()))]
#[case::timetz(
    Type::TIMETZ,
    [3_600_000_000i64.to_be_bytes().as_slice(), (-3600i32).to_be_bytes().as_slice()].concat(),
    Value::Time("01:00:00+01:00".into())
)]
#[case::timestamp(
    Type::TIMESTAMP,
    (9814i64 * 86_400_000_000 + 36_000_000_000).to_be_bytes().to_vec(),
    Value::Timestamp("2026-11-14T10:00:00".into())
)]
#[case::timestamptz(
    Type::TIMESTAMPTZ,
    (-1i64).to_be_bytes().to_vec(),
    Value::TimestampTz("1999-12-31T23:59:59.999999+00:00".into())
)]
#[case::timestamp_neg_infinity(Type::TIMESTAMP, i64::MIN.to_be_bytes().to_vec(), Value::Timestamp("-infinity".into()))]
#[case::interval(
    Type::INTERVAL,
    [14_706_000_000i64.to_be_bytes().as_slice(), 3i32.to_be_bytes().as_slice(), 14i32.to_be_bytes().as_slice()].concat(),
    Value::Interval("1 year 2 mons 3 days 04:05:06".into())
)]
#[case::interval_zero(Type::INTERVAL, vec![0; 16], Value::Interval("00:00:00".into()))]
#[case::interval_negative_time(
    Type::INTERVAL,
    [(-1_000_000i64).to_be_bytes().as_slice(), 1i32.to_be_bytes().as_slice(), 0i32.to_be_bytes().as_slice()].concat(),
    Value::Interval("1 day -00:00:01".into())
)]
#[case::inet(Type::INET, vec![2, 32, 0, 4, 10, 0, 0, 1], described("inet", "10.0.0.1"))]
#[case::inet_with_prefix(Type::INET, vec![2, 24, 0, 4, 10, 0, 0, 1], described("inet", "10.0.0.1/24"))]
#[case::cidr(Type::CIDR, vec![2, 32, 1, 4, 10, 0, 0, 1], described("cidr", "10.0.0.1/32"))]
#[case::inet6(
    Type::INET,
    [vec![3, 128, 0, 16], vec![0x20, 0x01, 0x0d, 0xb8], vec![0; 11], vec![1]].concat(),
    described("inet", "2001:db8::1")
)]
#[case::macaddr(Type::MACADDR, vec![8, 0, 0x2b, 1, 2, 3], described("macaddr", "08:00:2b:01:02:03"))]
#[case::varbit(Type::VARBIT, vec![0, 0, 0, 5, 0b1010_1000], described("varbit", "10101"))]
#[case::pg_lsn(Type::PG_LSN, 0x0000_0001_0000_00ffu64.to_be_bytes().to_vec(), described("pg_lsn", "1/FF"))]
fn decodes_scalars(#[case] ty: Type, #[case] bytes: Vec<u8>, #[case] expected: Value) {
    assert_eq!(decode(&ty, Some(&bytes)), expected);
}

#[rstest]
#[case::integer(numeric(0, 0, 0, &[42]), "42")]
#[case::zero(numeric(0, 0, 0, &[]), "0")]
#[case::fraction_with_scale(numeric(0, 0x4000, 4, &[12, 3400]), "-12.3400")]
// 0.000012 is the base-10000 groups 0000 and 1200 after the point.
#[case::leading_fraction_zeros(numeric(-2, 0, 6, &[1200]), "0.000012")]
#[case::thirty_digits(
    numeric(4, 0, 9, &[1234, 5678, 9012, 3456, 7890, 1234, 5678, 9000]),
    "12345678901234567890.123456789"
)]
#[case::trailing_zero_groups(numeric(2, 0, 0, &[1]), "100000000")]
#[case::nan(numeric(0, 0xC000, 0, &[]), "NaN")]
#[case::infinity(numeric(0, 0xD000, 0, &[]), "Infinity")]
#[case::negative_zero_is_zero(numeric(0, 0x4000, 0, &[]), "0")]
fn decodes_numeric_exactly(#[case] bytes: Vec<u8>, #[case] expected: &str) {
    assert_eq!(
        decode(&Type::NUMERIC, Some(&bytes)),
        Value::Decimal(expected.into())
    );
}

#[test]
fn null_is_null_for_every_type() {
    assert_eq!(decode(&Type::INT4, None), Value::Null);
    assert_eq!(decode(&Type::TEXT_ARRAY, None), Value::Null);
}

#[test]
fn decodes_one_dimensional_arrays_with_nulls() {
    let bytes = int4_array(&[3], &[Some(1), None, Some(3)]);

    let value = decode(&Type::INT4_ARRAY, Some(&bytes));

    assert_eq!(
        value,
        Value::Array(vec![Value::Int(1), Value::Null, Value::Int(3)])
    );
}

#[test]
fn decodes_multi_dimensional_arrays_as_nested_arrays() {
    let bytes = int4_array(&[2, 2], &[Some(1), Some(2), Some(3), Some(4)]);

    let value = decode(&Type::INT4_ARRAY, Some(&bytes));

    let row = |a, b| Value::Array(vec![Value::Int(a), Value::Int(b)]);
    assert_eq!(value, Value::Array(vec![row(1, 2), row(3, 4)]));
}

#[test]
fn decodes_empty_arrays() {
    let bytes = [
        0i32.to_be_bytes(),
        0i32.to_be_bytes(),
        Type::INT4.oid().to_be_bytes().map(|b| b),
    ]
    .concat();

    assert_eq!(
        decode(&Type::INT4_ARRAY, Some(&bytes)),
        Value::Array(vec![])
    );
}

#[test]
fn decodes_ranges_as_text() {
    let lower = 1i32.to_be_bytes();
    let upper = 5i32.to_be_bytes();
    let bytes = [
        vec![0x02],
        element_bytes(Some(&lower)),
        element_bytes(Some(&upper)),
    ]
    .concat();

    assert_eq!(
        decode(&Type::INT4_RANGE, Some(&bytes)),
        described("int4range", "[1,5)")
    );
}

#[test]
fn decodes_unbounded_and_empty_ranges() {
    let upper = 10i32.to_be_bytes();
    let unbounded = [vec![0x08 | 0x04], element_bytes(Some(&upper))].concat();

    assert_eq!(
        decode(&Type::INT4_RANGE, Some(&unbounded)),
        described("int4range", "(,10]")
    );
    assert_eq!(
        decode(&Type::INT4_RANGE, Some(&[0x01])),
        described("int4range", "empty")
    );
}

#[test]
fn decodes_anonymous_records() {
    let int = 7i32.to_be_bytes();
    let mut bytes = 3i32.to_be_bytes().to_vec();
    bytes.extend(Type::INT4.oid().to_be_bytes());
    bytes.extend(element_bytes(Some(&int)));
    bytes.extend(Type::TEXT.oid().to_be_bytes());
    bytes.extend(element_bytes(Some(b"a b")));
    bytes.extend(Type::TEXT.oid().to_be_bytes());
    bytes.extend(element_bytes(None));

    assert_eq!(
        decode(&Type::RECORD, Some(&bytes)),
        described("record", "(7,\"a b\",)")
    );
}

#[test]
fn enums_and_domains_decode_as_their_labels_and_base_types() {
    let mood = Type::new(
        "mood".into(),
        90_001,
        Kind::Enum(vec!["ok".into()]),
        "public".into(),
    );
    let positive = Type::new(
        "positive".into(),
        90_002,
        Kind::Domain(Type::INT4),
        "public".into(),
    );

    assert_eq!(decode(&mood, Some(b"ok")), text("ok"));
    assert_eq!(decode(&positive, Some(&5i32.to_be_bytes())), Value::Int(5));
    assert_eq!(category(&positive), TypeCategory::Integer);
}

#[test]
fn decodes_the_hstore_extension() {
    let hstore = Type::new("hstore".into(), 90_003, Kind::Simple, "public".into());
    let mut bytes = 2i32.to_be_bytes().to_vec();
    bytes.extend(element_bytes(Some(b"a")));
    bytes.extend(element_bytes(Some(b"1")));
    bytes.extend(element_bytes(Some(b"b")));
    bytes.extend(element_bytes(None));

    assert_eq!(
        decode(&hstore, Some(&bytes)),
        described("hstore", "\"a\"=>\"1\", \"b\"=>NULL")
    );
}

#[test]
fn unknown_types_and_malformed_payloads_fall_back_to_hex() {
    let geometry = Type::new("geometry".into(), 90_004, Kind::Simple, "public".into());
    let expected = |name: &str, hex: &str| Value::Unknown {
        type_name: name.into(),
        text: None,
        hex: Some(hex.into()),
    };

    assert_eq!(
        decode(&geometry, Some(&[1, 2])),
        expected("geometry", "0102")
    );
    assert_eq!(decode(&Type::INT4, Some(&[1, 2])), expected("int4", "0102"));
    assert_eq!(decode(&Type::TEXT, Some(&[0xff])), expected("text", "ff"));
    assert_eq!(
        decode(&Type::INT4_ARRAY, Some(&[0, 0])),
        expected("_int4", "0000")
    );
}

#[rstest]
#[case(Type::BOOL, TypeCategory::Bool)]
#[case(Type::INT8, TypeCategory::Integer)]
#[case(Type::FLOAT8, TypeCategory::Float)]
#[case(Type::NUMERIC, TypeCategory::Decimal)]
#[case(Type::VARCHAR, TypeCategory::Text)]
#[case(Type::JSONB, TypeCategory::Json)]
#[case(Type::XML, TypeCategory::Xml)]
#[case(Type::BYTEA, TypeCategory::Binary)]
#[case(Type::TIMESTAMPTZ, TypeCategory::Temporal)]
#[case(Type::UUID, TypeCategory::Uuid)]
#[case(Type::INT4_ARRAY, TypeCategory::Array)]
#[case(Type::INT4_RANGE, TypeCategory::Other)]
#[case(Type::INET, TypeCategory::Other)]
fn categorises_types(#[case] ty: Type, #[case] expected: TypeCategory) {
    assert_eq!(category(&ty), expected);
}
