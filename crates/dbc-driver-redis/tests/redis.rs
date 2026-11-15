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

#[tokio::test]
#[ignore = "needs Docker"]
async fn the_key_browser_scans_keys_and_reads_every_type() {
    let server = server(None).await;
    let connection = RedisDriver
        .connect(&config(&server, None, None).await)
        .await
        .unwrap();
    for command in [
        "SET app:greeting hello",
        "PEXPIRE app:greeting 600000",
        "HSET app:user:1 name Ada lang en",
        "RPUSH app:queue a b c",
        "SADD app:tags x y",
        "ZADD app:scores 1.5 one 2 two",
        "XADD app:events 1-1 kind created",
        "SET other 1",
        r#"SET "app:bin\xff" raw"#,
    ] {
        rows(&connection, command).await;
    }
    let store = connection.key_value().unwrap();

    let mut keys = Vec::new();
    let mut cursor = None;
    loop {
        let page = store
            .scan_keys("app:*", cursor.as_deref(), 2)
            .await
            .unwrap();
        keys.extend(page.keys);
        cursor = page.cursor;
        if cursor.is_none() {
            break;
        }
    }
    keys.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    let names: Vec<(&str, &str)> = keys
        .iter()
        .map(|k| (k.key.name.as_str(), k.key_type.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("app:bin\u{fffd}", "string"),
            ("app:events", "stream"),
            ("app:greeting", "string"),
            ("app:queue", "list"),
            ("app:scores", "zset"),
            ("app:tags", "set"),
            ("app:user:1", "hash"),
        ]
    );

    let greeting = store.read_key(b"app:greeting", 100).await.unwrap();
    assert!(greeting.ttl_ms.is_some_and(|t| t > 0 && t <= 600_000));
    assert_eq!(greeting.rows.rows, vec![vec![Value::Text("hello".into())]]);
    let user = store.read_key(b"app:user:1", 100).await.unwrap();
    assert_eq!(
        (user.key_type.as_str(), user.length, user.rows.rows.len()),
        ("hash", 2, 2)
    );
    let queue = store.read_key(b"app:queue", 2).await.unwrap();
    assert_eq!(
        queue.rows.rows[1],
        vec![Value::Int(1), Value::Text("b".into())]
    );
    assert!(queue.rows.truncated);
    let scores = store.read_key(b"app:scores", 100).await.unwrap();
    assert_eq!(
        scores.rows.rows[0],
        vec![Value::Text("one".into()), Value::Float(1.5)]
    );
    let events = store.read_key(b"app:events", 100).await.unwrap();
    assert_eq!(events.rows.rows[0][0], Value::Text("1-1".into()));
    let binary = keys
        .iter()
        .find(|k| k.key.name.starts_with("app:bin"))
        .unwrap();
    let raw = dbc_core::kv::KeyName::decode(&binary.key.id).unwrap();
    assert_eq!(raw, b"app:bin\xff");
    assert_eq!(
        store.read_key(&raw, 10).await.unwrap().rows.rows[0][0],
        Value::Text("raw".into())
    );
    let missing = store.read_key(b"nope", 10).await.unwrap_err();
    assert_eq!(missing.code, DbErrorCode::NotFound);
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn edits_change_values_and_report_what_was_gone() {
    use dbc_core::kv::KeyEdit;

    let server = server(None).await;
    let connection = RedisDriver
        .connect(&config(&server, None, None).await)
        .await
        .unwrap();
    rows(&connection, "SET s old PX 600000").await;
    let store = connection.key_value().unwrap();
    let apply = |key: &'static [u8], edit: KeyEdit| async move { store.edit_key(key, &edit).await };

    apply(
        b"s",
        KeyEdit::SetString {
            value: "new".into(),
        },
    )
    .await
    .unwrap();
    apply(
        b"h",
        KeyEdit::HashSet {
            field: "f".into(),
            value: "v".into(),
        },
    )
    .await
    .unwrap();
    apply(
        b"l",
        KeyEdit::ListPush {
            value: "a".into(),
            head: false,
        },
    )
    .await
    .unwrap();
    apply(
        b"l",
        KeyEdit::ListPush {
            value: "b".into(),
            head: false,
        },
    )
    .await
    .unwrap();
    apply(
        b"l",
        KeyEdit::ListSet {
            index: 0,
            value: "A".into(),
        },
    )
    .await
    .unwrap();
    apply(b"l", KeyEdit::ListRemove { value: "b".into() })
        .await
        .unwrap();
    apply(
        b"z",
        KeyEdit::ZsetAdd {
            member: "m".into(),
            score: 2.5,
        },
    )
    .await
    .unwrap();
    apply(
        b"h",
        KeyEdit::Rename {
            new_name: "h2".into(),
        },
    )
    .await
    .unwrap();
    apply(b"h2", KeyEdit::Expire { ttl_ms: None })
        .await
        .unwrap();

    let string = store.read_key(b"s", 10).await.unwrap();
    assert_eq!(string.rows.rows[0][0], Value::Text("new".into()));
    assert!(string.ttl_ms.is_some(), "KEEPTTL keeps the expiry");
    assert_eq!(
        store.read_key(b"l", 10).await.unwrap().rows.rows,
        vec![vec![Value::Int(0), Value::Text("A".into())]]
    );
    assert_eq!(
        store.read_key(b"z", 10).await.unwrap().rows.rows[0][1],
        Value::Float(2.5)
    );
    assert_eq!(store.read_key(b"h2", 10).await.unwrap().key_type, "hash");

    let gone = apply(
        b"l",
        KeyEdit::ListRemove {
            value: "nope".into(),
        },
    )
    .await
    .unwrap_err();
    rows(&connection, "SET taken 1").await;
    let taken = apply(
        b"h2",
        KeyEdit::Rename {
            new_name: "taken".into(),
        },
    )
    .await
    .unwrap_err();
    let wrong_type = apply(
        b"s",
        KeyEdit::HashSet {
            field: "f".into(),
            value: "v".into(),
        },
    )
    .await
    .unwrap_err();
    apply(b"h2", KeyEdit::Delete).await.unwrap();
    let deleted_twice = apply(b"h2", KeyEdit::Delete).await.unwrap_err();

    assert_eq!(gone.code, DbErrorCode::NotFound);
    assert_eq!(taken.code, DbErrorCode::NotFound);
    assert_eq!(wrong_type.code, DbErrorCode::Execution);
    assert_eq!(deleted_twice.code, DbErrorCode::NotFound);
}
