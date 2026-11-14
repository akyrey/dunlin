//! Driver conformance suite against PostgreSQL. `cargo test -p dbc-driver-postgres -- --ignored`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use dbc_core::Value;
use dbc_core::conformance::{self, Harness, SyntaxErrorCase, TypeCase};
use dbc_core::driver::{ConnectionConfig, Driver, Endpoint};
use dbc_core::result::TypeCategory;
use dbc_core::schema::NamespaceRef;
use dbc_driver_postgres::PostgresDriver;
use secrecy::SecretString;

use common::Server;

struct PostgresHarness {
    server: Server,
}

fn described(type_name: &str, text: &str) -> Value {
    Value::Unknown {
        type_name: type_name.into(),
        text: Some(text.into()),
        hex: None,
    }
}

fn case(expression: &str, expected: Value, category: TypeCategory) -> TypeCase {
    TypeCase::new(format!("SELECT {expression}"), expected).with_category(category)
}

#[async_trait]
impl Harness for PostgresHarness {
    fn driver(&self) -> Arc<dyn Driver> {
        Arc::new(PostgresDriver)
    }

    async fn database(&self, read_only: bool) -> ConnectionConfig {
        let name = self.server.fresh_database().await;
        self.server.config(&name, read_only)
    }

    fn unreachable(&self) -> ConnectionConfig {
        let mut config = self.server.config("postgres", false);
        config.endpoint = Endpoint::Tcp {
            dial_host: "127.0.0.1".into(),
            dial_port: 1,
            server_name: "localhost".into(),
        };
        config
    }

    fn bad_credentials(&self) -> Option<ConnectionConfig> {
        let mut config = self.server.config("postgres", false);
        config.password = Some(SecretString::from("wrong"));
        Some(config)
    }

    fn type_cases(&self) -> Vec<TypeCase> {
        use TypeCategory as C;
        vec![
            case(
                "123456789012345678901234567890.123456789::numeric",
                Value::Decimal("123456789012345678901234567890.123456789".into()),
                C::Decimal,
            ),
            case(
                "'-0.00001200'::numeric",
                Value::Decimal("-0.00001200".into()),
                C::Decimal,
            ),
            case("'NaN'::numeric", Value::Decimal("NaN".into()), C::Decimal),
            case(
                "c_decimal FROM all_types WHERE id = 1",
                Value::Decimal("12345678901234567890.123456789".into()),
                C::Decimal,
            ),
            case(
                r#"'{"b": 1, "a": [true]}'::jsonb"#,
                Value::Json(r#"{"a": [true], "b": 1}"#.into()),
                C::Json,
            ),
            case(
                r#"'{"b":1,  "a":2}'::json"#,
                Value::Json(r#"{"b":1,  "a":2}"#.into()),
                C::Json,
            ),
            case("'<a>b</a>'::xml", Value::Xml("<a>b</a>".into()), C::Xml),
            case(
                "'2026-11-14 10:00:00.5+02'::timestamptz",
                Value::TimestampTz("2026-11-14T08:00:00.5+00:00".into()),
                C::Temporal,
            ),
            case(
                "'2026-11-14 10:00:00'::timestamp",
                Value::Timestamp("2026-11-14T10:00:00".into()),
                C::Temporal,
            ),
            case(
                "'infinity'::timestamp",
                Value::Timestamp("infinity".into()),
                C::Temporal,
            ),
            case(
                "'0044-03-15 BC'::date",
                Value::Date("-0043-03-15".into()),
                C::Temporal,
            ),
            case(
                "'23:59:59.999999'::time",
                Value::Time("23:59:59.999999".into()),
                C::Temporal,
            ),
            case(
                "'10:00:00-05'::timetz",
                Value::Time("10:00:00-05:00".into()),
                C::Temporal,
            ),
            case(
                "'1 year 2 months 3 days 04:05:06'::interval",
                Value::Interval("1 year 2 mons 3 days 04:05:06".into()),
                C::Temporal,
            ),
            case(
                "'-1 day 00:00:01'::interval",
                Value::Interval("-1 days +00:00:01".into()),
                C::Temporal,
            ),
            case(
                "'6f1c2d3e-0000-4000-8000-000000000001'::uuid",
                Value::Uuid("6f1c2d3e-0000-4000-8000-000000000001".into()),
                C::Uuid,
            ),
            case(
                r"'\xdeadbeef'::bytea",
                Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]),
                C::Binary,
            ),
            case(
                "'192.168.0.1/24'::inet",
                described("inet", "192.168.0.1/24"),
                C::Other,
            ),
            case("'happy'::mood", Value::Text("happy".into()), C::Text),
            case("5::positive", Value::Int(5), C::Integer),
            case(
                "ARRAY[1, NULL, 3]",
                Value::Array(vec![Value::Int(1), Value::Null, Value::Int(3)]),
                C::Array,
            ),
            case(
                "ARRAY['sad', 'ok']::mood[]",
                Value::Array(vec![Value::Text("sad".into()), Value::Text("ok".into())]),
                C::Array,
            ),
            case("int4range(1, 5)", described("int4range", "[1,5)"), C::Other),
            case(
                "ROW(1, 'a b')::pair",
                described("pair", "(1,\"a b\")"),
                C::Other,
            ),
            case(
                "'a=>1'::hstore",
                described("hstore", "\"a\"=>\"1\""),
                C::Other,
            ),
            case(
                "'MixedCase'::citext",
                Value::Text("MixedCase".into()),
                C::Text,
            ),
            case(
                "9223372036854775807::int8",
                Value::Int(i64::MAX),
                C::Integer,
            ),
            case(
                "9007199254740993::int8",
                Value::Int(9_007_199_254_740_993),
                C::Integer,
            ),
            case("'NaN'::float8", Value::Float(f64::NAN), C::Float),
            case("true", Value::Bool(true), C::Bool),
            case("''::text", Value::Text(String::new()), C::Text),
            case("NULL::int4", Value::Null, C::Integer),
        ]
    }

    fn fixture_namespace(&self, _config: &ConnectionConfig) -> NamespaceRef {
        NamespaceRef {
            catalog: None,
            schema: Some("public".into()),
        }
    }

    fn system_namespace(&self) -> Option<String> {
        Some("pg_catalog".into())
    }

    fn syntax_error(&self) -> SyntaxErrorCase {
        // PostgreSQL reports `FRM` (character 10, byte 9).
        SyntaxErrorCase {
            sql: "SELECT * FRM customers".into(),
            position: Some(9),
        }
    }

    fn slow_statement(&self) -> String {
        "SELECT pg_sleep(30)".into()
    }
}

async fn harness() -> PostgresHarness {
    PostgresHarness {
        server: Server::start().await,
    }
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn conformance_suite() {
    conformance::run_all(&harness().await).await.unwrap();
}
