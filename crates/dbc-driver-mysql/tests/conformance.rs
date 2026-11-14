//! Driver conformance suite against MySQL 8.4 and MariaDB 11.4.
//! `cargo test -p dbc-driver-mysql --test conformance -- --ignored`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use dbc_core::Value;
use dbc_core::conformance::{self, Harness, MYSQL_FIXTURE, SyntaxErrorCase, TypeCase};
use dbc_core::driver::{ConnectionConfig, Driver, Endpoint, TlsConfig};
use dbc_core::result::TypeCategory;
use dbc_driver_mysql::MysqlDriver;
use mysql_async::prelude::Queryable;
use mysql_async::{Conn, Opts};
use secrecy::SecretString;
use testcontainers_modules::mariadb::Mariadb;
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, Image, ImageExt};

const TYPE_TABLE: &str = "
    SET SESSION sql_mode = '';
    CREATE TABLE mysql_types (
        id INT PRIMARY KEY, t_tiny TINYINT, t_tiny1 TINYINT(1), t_utiny TINYINT UNSIGNED,
        t_small SMALLINT, t_med MEDIUMINT, t_int INT, t_uint INT UNSIGNED, t_big BIGINT,
        t_ubig BIGINT UNSIGNED, t_dec DECIMAL(40, 10), t_float FLOAT, t_double DOUBLE, t_bit BIT(5),
        t_date DATE, t_zero_date DATE, t_time TIME, t_datetime DATETIME(3), t_zero_datetime DATETIME,
        t_ts TIMESTAMP NULL, t_year YEAR, t_json JSON, t_enum ENUM('a', 'b'), t_set SET('x', 'y'),
        t_blob BLOB, t_binary BINARY(2), t_varbinary VARBINARY(10), t_text TEXT, t_char CHAR(3),
        t_varchar VARCHAR(10)
    );
    INSERT INTO mysql_types VALUES (
        1, -128, 1, 255, -32768, -8388608, -2147483648, 4294967295, -9223372036854775808,
        18446744073709551615, 123456789012345678901234567890.1234567890, 1.5, -2.25, b'10101',
        '2026-11-14', '0000-00-00', '-838:59:59', '2026-11-14 10:00:00.123', '0000-00-00 00:00:00',
        '2026-11-14 10:00:00', 2026, '{\"a\": 1}', 'b', 'x,y', X'00FF', X'0102', X'DEADBEEF',
        'héllo', 'ab', ''
    );
";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flavour {
    Mysql,
    Mariadb,
}

struct MysqlHarness {
    port: u16,
    flavour: Flavour,
    counter: AtomicUsize,
}

impl MysqlHarness {
    async fn admin(&self, database: &str) -> Conn {
        let url = format!("mysql://root@127.0.0.1:{}/{database}", self.port);
        Conn::new(Opts::from_url(&url).unwrap()).await.unwrap()
    }

    fn config(&self, database: &str, read_only: bool) -> ConnectionConfig {
        ConnectionConfig {
            endpoint: Endpoint::Tcp {
                dial_host: "127.0.0.1".into(),
                dial_port: self.port,
                server_name: "localhost".into(),
            },
            database: Some(database.into()),
            user: Some("root".into()),
            password: None,
            tls: TlsConfig::default(),
            read_only,
            options: Default::default(),
        }
    }
}

fn column(name: &str, expected: Value, category: TypeCategory) -> TypeCase {
    TypeCase::new(format!("SELECT {name} FROM mysql_types"), expected).with_category(category)
}

#[async_trait]
impl Harness for MysqlHarness {
    fn driver(&self) -> Arc<dyn Driver> {
        Arc::new(MysqlDriver)
    }

    async fn database(&self, read_only: bool) -> ConnectionConfig {
        let name = format!(
            "conformance_{}",
            self.counter.fetch_add(1, Ordering::SeqCst)
        );
        let mut admin = self.admin("mysql").await;
        admin
            .query_drop(format!("CREATE DATABASE {name} CHARACTER SET utf8mb4"))
            .await
            .unwrap();
        let mut conn = self.admin(&name).await;
        conn.query_drop(MYSQL_FIXTURE).await.unwrap();
        conn.query_drop(TYPE_TABLE).await.unwrap();
        self.config(&name, read_only)
    }

    fn unreachable(&self) -> ConnectionConfig {
        let mut config = self.config("mysql", false);
        config.endpoint = Endpoint::Tcp {
            dial_host: "127.0.0.1".into(),
            dial_port: 1,
            server_name: "localhost".into(),
        };
        config
    }

    fn bad_credentials(&self) -> Option<ConnectionConfig> {
        let mut config = self.config("mysql", false);
        config.password = Some(SecretString::from("wrong"));
        Some(config)
    }

    fn type_cases(&self) -> Vec<TypeCase> {
        use TypeCategory as C;
        let text = |s: &str| Value::Text(s.into());
        let json = if self.flavour == Flavour::Mysql {
            column("t_json", Value::Json("{\"a\": 1}".into()), C::Json)
        } else {
            // MariaDB's JSON is an alias for LONGTEXT.
            column("t_json", text("{\"a\": 1}"), C::Text)
        };
        vec![
            column("t_tiny", Value::Int(-128), C::Integer),
            column("t_tiny1", Value::Int(1), C::Integer),
            column("t_utiny", Value::Int(255), C::Integer),
            column("t_small", Value::Int(-32768), C::Integer),
            column("t_med", Value::Int(-8_388_608), C::Integer),
            column("t_int", Value::Int(i64::from(i32::MIN)), C::Integer),
            column("t_uint", Value::Int(i64::from(u32::MAX)), C::Integer),
            column("t_big", Value::Int(i64::MIN), C::Integer),
            column("t_ubig", Value::UInt(u64::MAX), C::Integer),
            column(
                "t_dec",
                Value::Decimal("123456789012345678901234567890.1234567890".into()),
                C::Decimal,
            ),
            column("t_float", Value::Float(1.5), C::Float),
            column("t_double", Value::Float(-2.25), C::Float),
            column(
                "t_bit",
                Value::Unknown {
                    type_name: "BIT".into(),
                    text: Some("10101".into()),
                    hex: None,
                },
                C::Other,
            ),
            column("t_date", Value::Date("2026-11-14".into()), C::Temporal),
            column("t_zero_date", Value::Date("0000-00-00".into()), C::Temporal),
            column("t_time", Value::Time("-838:59:59".into()), C::Temporal),
            column(
                "t_datetime",
                Value::Timestamp("2026-11-14T10:00:00.123".into()),
                C::Temporal,
            ),
            column(
                "t_zero_datetime",
                Value::Timestamp("0000-00-00T00:00:00".into()),
                C::Temporal,
            ),
            column(
                "t_ts",
                Value::Timestamp("2026-11-14T10:00:00".into()),
                C::Temporal,
            ),
            column("t_year", Value::Int(2026), C::Integer),
            json,
            column("t_enum", text("b"), C::Text),
            column("t_set", text("x,y"), C::Text),
            column("t_blob", Value::Bytes(vec![0x00, 0xff]), C::Binary),
            column("t_binary", Value::Bytes(vec![0x01, 0x02]), C::Binary),
            column(
                "t_varbinary",
                Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]),
                C::Binary,
            ),
            column("t_text", text("héllo"), C::Text),
            column("t_char", text("ab"), C::Text),
            column("t_varchar", text(""), C::Text),
            TypeCase::new("SELECT 9007199254740993", Value::Int(9_007_199_254_740_993)),
            TypeCase::new("SELECT NULL", Value::Null),
        ]
    }

    fn syntax_error(&self) -> SyntaxErrorCase {
        // MySQL reports no position, only "near '...' at line 1".
        SyntaxErrorCase {
            sql: "SELECT * FRM customers".into(),
            position: None,
        }
    }

    fn read_only_rejects(&self) -> Vec<String> {
        // DDL commits implicitly, so a read-only transaction does not stop it; the backend's
        // classifier rejects DDL on read-only sessions before it is sent (SECURITY.md).
        [
            "INSERT INTO event_log (message) VALUES ('conformance')",
            "UPDATE orders SET paid = paid WHERE id = 10",
            "DELETE FROM event_log WHERE message IS NULL",
        ]
        .map(str::to_owned)
        .to_vec()
    }

    fn slow_statement(&self) -> String {
        "SELECT SLEEP(30)".into()
    }
}

async fn run<I: Image>(container: &ContainerAsync<I>, flavour: Flavour) {
    let port = container.get_host_port_ipv4(3306).await.unwrap();
    let harness = MysqlHarness {
        port,
        flavour,
        counter: AtomicUsize::new(0),
    };
    conformance::run_all(&harness).await.unwrap();
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn conformance_suite_mysql() {
    let container = Mysql::default().with_tag("8.4").start().await.unwrap();
    run(&container, Flavour::Mysql).await;
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn conformance_suite_mariadb() {
    let container = Mariadb::default().with_tag("11.4").start().await.unwrap();
    run(&container, Flavour::Mariadb).await;
}
