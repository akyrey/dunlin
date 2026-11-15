//! Redis replies as result grids: a scalar is one `value` cell, an array or set one row per
//! element, a map (RESP3) one row per entry. Nested replies are shown as JSON.

use dbc_core::result::{ColumnMeta, ResultSet, TypeCategory};
use dbc_core::{DbError, DbErrorCode, Value};
use redis::Value as Reply;
use serde_json::Value as Json;

fn column(name: &str, category: TypeCategory) -> ColumnMeta {
    ColumnMeta {
        name: name.to_owned(),
        type_name: String::new(),
        category,
        nullable: None,
    }
}

/// The grid for one reply; at most `row_limit` rows.
pub fn to_result(reply: Reply, row_limit: usize) -> Result<ResultSet, DbError> {
    let (columns, rows): (Vec<ColumnMeta>, Vec<Vec<Value>>) = match reply {
        Reply::Attribute { data, .. } => return to_result(*data, row_limit),
        Reply::ServerError(error) => return Err(server_error(&error)),
        Reply::Array(items) | Reply::Set(items) | Reply::Push { data: items, .. } => (
            vec![
                column("#", TypeCategory::Integer),
                column("value", TypeCategory::Text),
            ],
            items
                .into_iter()
                .enumerate()
                .map(|(i, item)| {
                    vec![
                        Value::Int(i64::try_from(i + 1).unwrap_or(i64::MAX)),
                        cell(item),
                    ]
                })
                .collect(),
        ),
        Reply::Map(entries) => (
            vec![
                column("key", TypeCategory::Text),
                column("value", TypeCategory::Text),
            ],
            entries
                .into_iter()
                .map(|(k, v)| vec![cell(k), cell(v)])
                .collect(),
        ),
        scalar => (
            vec![column("value", TypeCategory::Text)],
            vec![vec![cell(scalar)]],
        ),
    };
    let truncated = rows.len() > row_limit;
    Ok(ResultSet {
        columns,
        rows: rows.into_iter().take(row_limit).collect(),
        truncated,
    })
}

pub fn server_error(error: &redis::ServerError) -> DbError {
    let code = match error.code() {
        "NOAUTH" | "WRONGPASS" => DbErrorCode::Auth,
        "NOPERM" => DbErrorCode::Permission,
        "READONLY" => DbErrorCode::ReadOnly,
        _ if error.code() == "ERR"
            && error
                .details()
                .is_some_and(|d| d.contains("unknown command")) =>
        {
            DbErrorCode::Syntax
        }
        _ => DbErrorCode::Execution,
    };
    DbError::new(code, error.to_string())
}

/// One reply as a grid cell.
pub fn cell(reply: Reply) -> Value {
    match reply {
        Reply::Nil => Value::Null,
        Reply::Int(n) => Value::Int(n),
        Reply::Double(f) => Value::Float(f),
        Reply::Boolean(b) => Value::Bool(b),
        Reply::Okay => Value::Text("OK".into()),
        Reply::SimpleString(s) => Value::Text(s),
        Reply::VerbatimString { text, .. } => Value::Text(text),
        Reply::BulkString(bytes) => bytes_cell(bytes),
        Reply::BigNumber(digits) => Value::Decimal(String::from_utf8_lossy(&digits).into_owned()),
        Reply::Attribute { data, .. } => cell(*data),
        Reply::ServerError(error) => Value::Text(format!("(error) {error}")),
        nested => Value::Json(json(nested).to_string()),
    }
}

/// Text when it is UTF-8, else bytes (shown as hex).
pub fn bytes_cell(bytes: Vec<u8>) -> Value {
    match String::from_utf8(bytes) {
        Ok(text) => Value::Text(text),
        Err(error) => Value::Bytes(error.into_bytes()),
    }
}

fn json(reply: Reply) -> Json {
    match reply {
        Reply::Nil => Json::Null,
        Reply::Int(n) => Json::from(n),
        Reply::Double(f) => serde_json::Number::from_f64(f).map_or(Json::Null, Json::Number),
        Reply::Boolean(b) => Json::Bool(b),
        Reply::Okay => Json::from("OK"),
        Reply::SimpleString(s) => Json::from(s),
        Reply::VerbatimString { text, .. } => Json::from(text),
        Reply::BulkString(bytes) => Json::from(String::from_utf8_lossy(&bytes).into_owned()),
        Reply::BigNumber(digits) => Json::from(String::from_utf8_lossy(&digits).into_owned()),
        Reply::Array(items) | Reply::Set(items) | Reply::Push { data: items, .. } => {
            Json::Array(items.into_iter().map(json).collect())
        }
        Reply::Map(entries) => Json::Object(
            entries
                .into_iter()
                .map(|(k, v)| (key_text(k), json(v)))
                .collect(),
        ),
        Reply::Attribute { data, .. } => json(*data),
        Reply::ServerError(error) => Json::from(format!("(error) {error}")),
        other => Json::from(format!("{other:?}")),
    }
}

fn key_text(reply: Reply) -> String {
    match json(reply) {
        Json::String(s) => s,
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn scalars_arrays_and_maps_become_grids() {
        let ok = to_result(Reply::Okay, 10).unwrap();
        let list = to_result(
            Reply::Array(vec![
                Reply::BulkString(b"a".to_vec()),
                Reply::Int(2),
                Reply::Nil,
            ]),
            2,
        )
        .unwrap();
        let map = to_result(
            Reply::Map(vec![(
                Reply::BulkString(b"f".to_vec()),
                Reply::BulkString(vec![0xff]),
            )]),
            10,
        )
        .unwrap();

        assert_eq!(ok.rows, vec![vec![Value::Text("OK".into())]]);
        assert_eq!(list.columns[0].name, "#");
        assert_eq!(
            list.rows,
            vec![
                vec![Value::Int(1), Value::Text("a".into())],
                vec![Value::Int(2), Value::Int(2)],
            ]
        );
        assert!(list.truncated);
        assert_eq!(
            map.rows,
            vec![vec![Value::Text("f".into()), Value::Bytes(vec![0xff])]]
        );
    }

    #[test]
    fn nested_replies_are_json() {
        let nested = cell(Reply::Array(vec![
            Reply::Int(1),
            Reply::Map(vec![(
                Reply::SimpleString("k".into()),
                Reply::Boolean(true),
            )]),
        ]));

        assert_eq!(nested, Value::Json(r#"[1,{"k":true}]"#.into()));
    }

    /// A server error as the parser reads it off the wire.
    fn server(line: &str) -> redis::ServerError {
        match redis::parse_redis_value(format!("-{line}\r\n").as_bytes()).unwrap() {
            Reply::ServerError(error) => error,
            other => panic!("not an error: {other:?}"),
        }
    }

    #[test]
    fn server_errors_map_to_codes() {
        let cases = [
            (
                "WRONGPASS invalid username-password pair",
                DbErrorCode::Auth,
            ),
            ("NOAUTH Authentication required.", DbErrorCode::Auth),
            (
                "NOPERM this user has no permissions",
                DbErrorCode::Permission,
            ),
            (
                "READONLY You can't write against a read only replica.",
                DbErrorCode::ReadOnly,
            ),
            (
                "ERR unknown command 'NOPE', with args beginning with:",
                DbErrorCode::Syntax,
            ),
            (
                "ERR value is not an integer or out of range",
                DbErrorCode::Execution,
            ),
            (
                "WRONGTYPE Operation against a key holding the wrong kind of value",
                DbErrorCode::Execution,
            ),
        ];
        for (line, code) in cases {
            assert_eq!(server_error(&server(line)).code, code, "{line}");
        }
        let refused = to_result(Reply::ServerError(server("ERR bad")), 10).unwrap_err();
        assert_eq!(refused.code, DbErrorCode::Execution);
    }

    #[test]
    fn every_scalar_becomes_a_cell() {
        let cases = [
            (Reply::Nil, Value::Null),
            (Reply::Int(-4), Value::Int(-4)),
            (Reply::Double(1.5), Value::Float(1.5)),
            (Reply::Boolean(false), Value::Bool(false)),
            (
                Reply::SimpleString("PONG".into()),
                Value::Text("PONG".into()),
            ),
            (
                Reply::VerbatimString {
                    format: redis::VerbatimFormat::Text,
                    text: "verbatim".into(),
                },
                Value::Text("verbatim".into()),
            ),
            (
                Reply::BigNumber(b"123456789012345678901234567890".to_vec()),
                Value::Decimal("123456789012345678901234567890".into()),
            ),
            (
                Reply::Attribute {
                    data: Box::new(Reply::Int(7)),
                    attributes: Vec::new(),
                },
                Value::Int(7),
            ),
        ];
        for (reply, expected) in cases {
            assert_eq!(cell(reply.clone()), expected, "{reply:?}");
        }
        assert!(
            matches!(cell(Reply::ServerError(server("ERR no"))), Value::Text(t) if t.starts_with("(error)"))
        );
    }

    #[test]
    fn sets_pushes_and_attributes_become_grids_too() {
        let set = to_result(Reply::Set(vec![Reply::Int(1)]), 10).unwrap();
        let attributed = to_result(
            Reply::Attribute {
                data: Box::new(Reply::Array(vec![Reply::Int(1), Reply::Int(2)])),
                attributes: Vec::new(),
            },
            10,
        )
        .unwrap();

        assert_eq!(set.rows, vec![vec![Value::Int(1), Value::Int(1)]]);
        assert_eq!(attributed.rows.len(), 2);
        assert!(!attributed.truncated);
    }

    #[test]
    fn nested_values_of_every_kind_are_json() {
        let nested = cell(Reply::Array(vec![
            Reply::Nil,
            Reply::Double(f64::NAN),
            Reply::Double(0.5),
            Reply::Okay,
            Reply::BulkString(b"bulk".to_vec()),
            Reply::BigNumber(b"99".to_vec()),
            Reply::VerbatimString {
                format: redis::VerbatimFormat::Markdown,
                text: "md".into(),
            },
            Reply::Set(vec![Reply::Boolean(true)]),
            Reply::Map(vec![(Reply::Int(1), Reply::SimpleString("one".into()))]),
            Reply::ServerError(server("ERR inner")),
        ]));

        let Value::Json(text) = nested else {
            panic!("expected JSON");
        };
        let parsed: Json = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed[0], Json::Null);
        assert_eq!(parsed[1], Json::Null, "NaN has no JSON form");
        assert_eq!(parsed[2], Json::from(0.5));
        assert_eq!(parsed[3], Json::from("OK"));
        assert_eq!(parsed[4], Json::from("bulk"));
        assert_eq!(parsed[5], Json::from("99"));
        assert_eq!(parsed[6], Json::from("md"));
        assert_eq!(parsed[7], Json::Array(vec![Json::Bool(true)]));
        assert_eq!(
            parsed[8]["1"],
            Json::from("one"),
            "non-text map keys become text"
        );
        assert!(parsed[9].as_str().unwrap().starts_with("(error)"));
    }
}
