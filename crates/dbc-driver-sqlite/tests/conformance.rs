#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use dbc_core::Value;
use dbc_core::conformance::{self, Harness, SyntaxErrorCase, TypeCase};
use dbc_core::driver::{ConnectionConfig, Driver};
use dbc_core::result::TypeCategory;
use dbc_driver_sqlite::SqliteDriver;

use common::{TempDatabases, config, creating};

#[derive(Default)]
struct SqliteHarness {
    databases: TempDatabases,
}

#[async_trait]
impl Harness for SqliteHarness {
    fn driver(&self) -> Arc<dyn Driver> {
        Arc::new(SqliteDriver)
    }

    async fn database(&self, read_only: bool) -> ConnectionConfig {
        config(self.databases.fixture(), read_only)
    }

    fn unreachable(&self) -> ConnectionConfig {
        creating(self.databases.dir().join("missing-dir").join("x.db"))
    }

    fn bad_credentials(&self) -> Option<ConnectionConfig> {
        None
    }

    fn type_cases(&self) -> Vec<TypeCase> {
        let column = |name: &str| format!("SELECT {name} FROM all_types WHERE id = 1");
        vec![
            TypeCase::new("SELECT NULL", Value::Null),
            TypeCase::new("SELECT 9007199254740993", Value::Int(9_007_199_254_740_993))
                .with_category(TypeCategory::Integer),
            TypeCase::new("SELECT 1.5", Value::Float(1.5)).with_category(TypeCategory::Float),
            TypeCase::new("SELECT ''", Value::Text(String::new())),
            TypeCase::new("SELECT 'héllo'", Value::Text("héllo".into())),
            TypeCase::new("SELECT X'FFFE00'", Value::Bytes(vec![0xff, 0xfe, 0x00]))
                .with_category(TypeCategory::Binary),
            TypeCase::new(
                "SELECT CAST(X'FFFE' AS TEXT)",
                Value::Bytes(vec![0xff, 0xfe]),
            ),
            TypeCase::new(column("c_bool"), Value::Int(1)).with_category(TypeCategory::Bool),
            TypeCase::new(column("c_bigint"), Value::Int(i64::MAX))
                .with_category(TypeCategory::Integer),
            TypeCase::new(column("c_real"), Value::Float(1.5)).with_category(TypeCategory::Float),
            TypeCase::new(column("c_text"), Value::Text("héllo".into()))
                .with_category(TypeCategory::Text),
            TypeCase::new(column("c_blob"), Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]))
                .with_category(TypeCategory::Binary),
            TypeCase::new(column("c_json"), Value::Text("{\"a\": [1, 2]}".into()))
                .with_category(TypeCategory::Json),
            TypeCase::new(column("c_xml"), Value::Text("<a>b</a>".into()))
                .with_category(TypeCategory::Xml),
            TypeCase::new(
                column("c_uuid"),
                Value::Text("6f1c2d3e-0000-4000-8000-000000000001".into()),
            )
            .with_category(TypeCategory::Uuid),
            TypeCase::new(column("c_date"), Value::Text("2026-11-14".into()))
                .with_category(TypeCategory::Temporal),
            TypeCase::new("SELECT c_text FROM all_types WHERE id = 2", Value::Null)
                .with_category(TypeCategory::Text),
        ]
    }

    fn syntax_error(&self) -> SyntaxErrorCase {
        SyntaxErrorCase {
            sql: "SELECT * FRM customers".into(),
            position: Some(9),
        }
    }
}

#[tokio::test]
async fn connect_group() {
    conformance::connect_group(&SqliteHarness::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn type_round_trip_group() {
    conformance::type_round_trip_group(&SqliteHarness::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn row_limit_group() {
    conformance::row_limit_group(&SqliteHarness::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn execute_group() {
    conformance::execute_group(&SqliteHarness::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn read_only_group() {
    conformance::read_only_group(&SqliteHarness::default())
        .await
        .unwrap();
}
