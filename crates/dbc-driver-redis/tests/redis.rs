//! The driver against a real Redis server.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::Arc;
use std::time::Duration;

use dbc_core::driver::{Connection, ConnectionConfig, Driver, Endpoint, TlsConfig};
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_core::{DbErrorCode, Value};
use dbc_driver_redis::RedisDriver;
use secrecy::SecretString;
use testcontainers_modules::redis::Redis;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

async fn server(password: Option<&str>) -> ContainerAsync<Redis> {
    let image = Redis::default().with_tag("7.4");
    match password {
        Some(p) => image
            .with_cmd(["redis-server", "--requirepass", p])
            .start()
            .await
            .unwrap(),
        None => image.start().await.unwrap(),
    }
}

async fn config(
    server: &ContainerAsync<Redis>,
    database: Option<&str>,
    password: Option<&str>,
) -> ConnectionConfig {
    let port = server.get_host_port_ipv4(6379).await.unwrap();
    ConnectionConfig {
        endpoint: Endpoint::Tcp {
            dial_host: "127.0.0.1".into(),
            dial_port: port,
            server_name: "localhost".into(),
        },
        database: database.map(str::to_owned),
        user: None,
        password: password.map(|p| SecretString::from(p.to_owned())),
        tls: TlsConfig::default(),
        read_only: false,
        options: Default::default(),
    }
}

async fn rows(connection: &Arc<dyn Connection>, command: &str) -> Vec<Vec<Value>> {
    match connection
        .execute(command, &ExecuteOptions::default())
        .await
        .unwrap()
    {
        StatementOutcome::Rows(result) => result.rows,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn commands_run_and_replies_become_grids() {
    let server = server(None).await;
    let connection = RedisDriver
        .connect(&config(&server, Some("2"), None).await)
        .await
        .unwrap();

    assert!(
        connection
            .server_version()
            .await
            .unwrap()
            .starts_with("Redis 7.4")
    );
    assert_eq!(
        rows(&connection, r#"SET "user:1 name" "Ada Lovelace""#).await,
        vec![vec![Value::Text("OK".into())]]
    );
    assert_eq!(
        rows(&connection, "GET 'user:1 name'").await,
        vec![vec![Value::Text("Ada Lovelace".into())]]
    );
    rows(&connection, "HSET h a 1 b 2").await;
    let hash = rows(&connection, "HGETALL h").await;
    assert_eq!(hash.len(), 2, "RESP3 maps are one row per field: {hash:?}");
    assert_eq!(hash[0].len(), 2);
    rows(&connection, "RPUSH l x y z").await;
    assert_eq!(
        rows(&connection, "LRANGE l 0 -1").await[2],
        vec![Value::Int(3), Value::Text("z".into())]
    );
    assert_eq!(
        rows(&connection, "GET missing").await,
        vec![vec![Value::Null]]
    );
    // The database number from the profile was selected.
    let other = RedisDriver
        .connect(&config(&server, Some("0"), None).await)
        .await
        .unwrap();
    assert_eq!(rows(&other, "DBSIZE").await, vec![vec![Value::Int(0)]]);

    let wrong = connection
        .execute("HGETALL", &ExecuteOptions::default())
        .await
        .unwrap_err();
    let unknown = connection
        .execute("NOPE x", &ExecuteOptions::default())
        .await
        .unwrap_err();
    let subscribe = connection
        .execute("SUBSCRIBE ch", &ExecuteOptions::default())
        .await
        .unwrap_err();
    assert_eq!(wrong.code, DbErrorCode::Execution);
    assert_eq!(unknown.code, DbErrorCode::Syntax);
    assert_eq!(subscribe.code, DbErrorCode::Unsupported);
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn passwords_are_checked() {
    let server = server(Some("s3cret")).await;

    let refused = RedisDriver
        .connect(&config(&server, None, Some("wrong")).await)
        .await
        .err()
        .unwrap();
    let missing = RedisDriver
        .connect(&config(&server, None, None).await)
        .await;
    let ok = RedisDriver
        .connect(&config(&server, None, Some("s3cret")).await)
        .await
        .unwrap();

    assert_eq!(refused.code, DbErrorCode::Auth, "{refused:?}");
    // The RESP3 handshake needs the password, so a missing one is refused when connecting.
    assert_eq!(missing.err().unwrap().code, DbErrorCode::Auth);
    ok.ping().await.unwrap();
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn cancelling_a_blocking_command_closes_the_connection() {
    let server = server(None).await;
    let connection = RedisDriver
        .connect(&config(&server, None, None).await)
        .await
        .unwrap();
    let cancel = connection.cancel_handle();

    let blocked = {
        let connection = Arc::clone(&connection);
        tokio::spawn(async move {
            connection
                .execute("BLPOP never 0", &ExecuteOptions::default())
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    cancel.cancel().await.unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(5), blocked)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(outcome.unwrap_err().code, DbErrorCode::Cancelled);
    assert!(connection.is_broken());
}
