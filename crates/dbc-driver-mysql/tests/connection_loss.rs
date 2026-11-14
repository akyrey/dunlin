//! The server ends our session: the next call reports `ConnectionLost`, and a new connection works.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dbc_core::driver::{ConnectionConfig, Driver, Endpoint, TlsConfig};
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_core::{DbErrorCode, Value};
use dbc_driver_mysql::MysqlDriver;
use mysql_async::prelude::Queryable;
use mysql_async::{Conn, Opts};
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
#[ignore = "needs Docker"]
async fn a_killed_connection_is_reported_as_connection_lost() {
    let container = Mysql::default().with_tag("8.4").start().await.unwrap();
    let port = container.get_host_port_ipv4(3306).await.unwrap();
    let config = ConnectionConfig {
        endpoint: Endpoint::Tcp {
            dial_host: "127.0.0.1".into(),
            dial_port: port,
            server_name: "localhost".into(),
        },
        database: None,
        user: Some("root".into()),
        password: None,
        tls: TlsConfig::default(),
        read_only: false,
        options: Default::default(),
    };
    let connection = MysqlDriver.connect(&config).await.unwrap();
    let options = ExecuteOptions::default();
    let StatementOutcome::Rows(id) = connection
        .execute("SELECT CONNECTION_ID()", &options)
        .await
        .unwrap()
    else {
        panic!("expected rows")
    };
    let id = match &id.rows[0][0] {
        Value::Int(i) => i.to_string(),
        Value::UInt(u) => u.to_string(),
        other => panic!("unexpected {other:?}"),
    };
    let mut admin = Conn::new(Opts::from_url(&format!("mysql://root@127.0.0.1:{port}")).unwrap())
        .await
        .unwrap();
    admin
        .query_drop(format!("KILL CONNECTION {id}"))
        .await
        .unwrap();

    let lost = connection
        .execute("SELECT 1", &options)
        .await
        .err()
        .map(|e| e.code);
    let fresh = MysqlDriver
        .connect(&config)
        .await
        .unwrap()
        .execute("SELECT 1", &options)
        .await;

    assert_eq!(lost, Some(DbErrorCode::ConnectionLost));
    assert!(fresh.is_ok());
}
