use rstest::rstest;
use serde_json::{Value as Json, json};

use super::*;

fn to_json(value: &Value) -> Json {
    serde_json::to_value(value).unwrap()
}

fn round_trip(value: &Value) -> Value {
    serde_json::from_value(to_json(value)).unwrap()
}

#[rstest]
#[case::null(Value::Null, json!({"t": "null"}))]
#[case::bool(Value::Bool(true), json!({"t": "bool", "v": true}))]
#[case::int(Value::Int(9_007_199_254_740_993), json!({"t": "int", "v": "9007199254740993"}))]
#[case::int_max(Value::Int(i64::MAX), json!({"t": "int", "v": "9223372036854775807"}))]
#[case::int_min(Value::Int(i64::MIN), json!({"t": "int", "v": "-9223372036854775808"}))]
#[case::uint_max(Value::UInt(u64::MAX), json!({"t": "uint", "v": "18446744073709551615"}))]
#[case::float(Value::Float(1.5), json!({"t": "float", "v": "1.5"}))]
#[case::float_large(Value::Float(1e300), json!({"t": "float", "v": "1e300"}))]
#[case::float_nan(Value::Float(f64::NAN), json!({"t": "float", "v": "NaN"}))]
#[case::float_inf(Value::Float(f64::INFINITY), json!({"t": "float", "v": "Infinity"}))]
#[case::float_neg_inf(Value::Float(f64::NEG_INFINITY), json!({"t": "float", "v": "-Infinity"}))]
#[case::decimal(
    Value::Decimal("12345678901234567890.123456789".into()),
    json!({"t": "decimal", "v": "12345678901234567890.123456789"})
)]
#[case::text(Value::Text("hello".into()), json!({"t": "text", "v": "hello"}))]
#[case::empty_text(Value::Text(String::new()), json!({"t": "text", "v": ""}))]
#[case::bytes(Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]), json!({"t": "bytes", "v": "3q2+7w==", "len": 4}))]
#[case::json(Value::Json("{\"a\": 1}".into()), json!({"t": "json", "v": "{\"a\": 1}"}))]
#[case::xml(Value::Xml("<a/>".into()), json!({"t": "xml", "v": "<a/>"}))]
#[case::uuid(Value::Uuid("6f1c".into()), json!({"t": "uuid", "v": "6f1c"}))]
#[case::date(Value::Date("2026-11-14".into()), json!({"t": "date", "v": "2026-11-14"}))]
#[case::time(Value::Time("10:00:00".into()), json!({"t": "time", "v": "10:00:00"}))]
#[case::timestamp(Value::Timestamp("2026-11-14T10:00:00".into()), json!({"t": "timestamp", "v": "2026-11-14T10:00:00"}))]
#[case::timestamptz(
    Value::TimestampTz("2026-11-14T10:00:00+01:00".into()),
    json!({"t": "timestamptz", "v": "2026-11-14T10:00:00+01:00"})
)]
#[case::interval(Value::Interval("1 day".into()), json!({"t": "interval", "v": "1 day"}))]
#[case::array(
    Value::Array(vec![Value::Int(1), Value::Null]),
    json!({"t": "array", "v": [{"t": "int", "v": "1"}, {"t": "null"}]})
)]
#[case::unknown(
    Value::Unknown { type_name: "geometry".into(), text: None, hex: Some("0101".into()) },
    json!({"t": "unknown", "type": "geometry", "hex": "0101"})
)]
fn serialises_to_documented_wire_format(#[case] value: Value, #[case] expected: Json) {
    let actual = to_json(&value);

    assert_eq!(actual, expected);
}

#[rstest]
#[case(Value::Null)]
#[case(Value::Bool(false))]
#[case(Value::Int(i64::MAX))]
#[case(Value::Int(i64::MIN))]
#[case(Value::UInt(u64::MAX))]
#[case(Value::Float(-0.1))]
#[case(Value::Float(f64::INFINITY))]
#[case(Value::Float(f64::NEG_INFINITY))]
#[case(Value::Float(f64::MIN_POSITIVE))]
#[case(Value::Decimal("-0.000000000000000001".into()))]
#[case(Value::Text(String::new()))]
#[case(Value::Text("héllo 🌍".into()))]
#[case(Value::Bytes(vec![]))]
#[case(Value::Bytes(vec![0xff, 0xfe, 0x00, 0xc3]))]
#[case(Value::Json("[1, 2]".into()))]
#[case(Value::Xml("<a>b</a>".into()))]
#[case(Value::Uuid("00000000-0000-0000-0000-000000000000".into()))]
#[case(Value::Date("2026-01-01".into()))]
#[case(Value::Time("23:59:59.999".into()))]
#[case(Value::Timestamp("2026-01-01T00:00:00".into()))]
#[case(Value::TimestampTz("2026-01-01T00:00:00Z".into()))]
#[case(Value::Interval("P1D".into()))]
#[case(Value::Array(vec![Value::Array(vec![Value::Text("x".into())]), Value::Null]))]
#[case(Value::Unknown { type_name: "inet".into(), text: Some("10.0.0.1".into()), hex: None })]
fn every_variant_round_trips(#[case] value: Value) {
    let decoded = round_trip(&value);

    assert_eq!(decoded, value);
}

#[test]
fn nan_round_trips_as_nan() {
    let decoded = round_trip(&Value::Float(f64::NAN));

    assert!(matches!(decoded, Value::Float(f) if f.is_nan()));
}

#[test]
fn empty_string_and_null_are_distinct_on_the_wire() {
    let empty = to_json(&Value::Text(String::new()));
    let null = to_json(&Value::Null);

    assert_ne!(empty, null);
}

#[test]
fn long_text_is_truncated_on_a_char_boundary_and_flagged() {
    let value = Value::Text("aé".repeat(10));

    let wire = value.into_wire(4);

    let expected = WireValue::Text(wire::TextPayload {
        v: "aéa".into(),
        truncated: true,
    });
    assert_eq!(wire, expected);
}

#[test]
fn text_within_limit_is_not_flagged() {
    let wire = Value::Text("abc".into()).into_wire(3);

    assert_eq!(
        serde_json::to_value(wire).unwrap(),
        json!({"t": "text", "v": "abc"})
    );
}

#[test]
fn long_bytes_are_truncated_and_keep_true_length() {
    let wire = Value::Bytes(vec![1, 2, 3, 4, 5]).into_wire(2);

    let json = serde_json::to_value(wire).unwrap();

    assert_eq!(
        json,
        json!({"t": "bytes", "v": "AQI=", "len": 5, "truncated": true})
    );
}

#[test]
fn truncation_applies_inside_arrays() {
    let value = Value::Array(vec![Value::Json("x".repeat(10))]);

    let json = serde_json::to_value(value.into_wire(3)).unwrap();

    assert_eq!(
        json,
        json!({"t": "array", "v": [{"t": "json", "v": "xxx", "truncated": true}]})
    );
}

#[rstest]
#[case::truncated_text(json!({"t": "text", "v": "abc", "truncated": true}))]
#[case::truncated_bytes(json!({"t": "bytes", "v": "AQI=", "len": 5, "truncated": true}))]
#[case::bad_int(json!({"t": "int", "v": "9223372036854775808"}))]
#[case::bad_uint(json!({"t": "uint", "v": "-1"}))]
#[case::bad_float(json!({"t": "float", "v": "one"}))]
#[case::bad_base64(json!({"t": "bytes", "v": "!!!", "len": 3}))]
#[case::len_mismatch(json!({"t": "bytes", "v": "AQI=", "len": 3}))]
#[case::unknown_tag(json!({"t": "money", "v": "1"}))]
fn rejects_invalid_input(#[case] input: Json) {
    let result = serde_json::from_value::<Value>(input);

    assert!(result.is_err());
}
