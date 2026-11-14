//! Phase 8 acceptance on MySQL and MariaDB: exported CSV re-imports with `LOAD DATA`, and
//! exported `INSERT` statements reproduce the rows. Needs Docker.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::Arc;

use dbc_core::dialect::Dialect;
use dbc_core::driver::{Connection, ConnectionConfig, Driver, Endpoint, TlsConfig};
use dbc_core::export::{CsvOptions, ExportFormat, ExportWriter};
use dbc_core::result::StreamEvent;
use dbc_core::schema::{NamespaceRef, TableRef};
use dbc_driver_mysql::MysqlDriver;
use mysql_async::prelude::Queryable;
use mysql_async::{Conn, Opts, OptsBuilder, WhiteListFsHandler};
use testcontainers_modules::mariadb::Mariadb;
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, Image, ImageExt};
use tokio::sync::mpsc;

const TYPES: &str = "id INT PRIMARY KEY, note TEXT, amount DECIMAL(38, 9), big BIGINT, ratio DOUBLE, \
                     flag TINYINT(1), data BLOB, doc JSON, at DATETIME(6), day DATE";
const COLUMNS: &str = "id, note, amount, big, ratio, flag, @data, doc, at, day";

const ROWS: &str = r#"
    INSERT INTO source VALUES
      (1, 'it''s, "quoted"; DROP TABLE x; --', 12345678901234567890.123456789, 9223372036854775807,
       1.5, 1, X'DEAD00', '{"a": [1, "b"]}', '2026-11-14 10:00:00.123456', '2026-11-14'),
      (2, 'line one\nline two \\ back', -0.5, -1, -2.25, 0, X'', 'null', NULL, NULL),
      (3, '', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL),
      (4, 'NULL', 0, 0, 0, NULL, NULL, '[]', '1000-01-01 00:00:00', '9999-12-31');"#;

async fn export(
    connection: &dyn Connection,
    dialect: Arc<dyn Dialect>,
    sql: &str,
    format: ExportFormat,
) -> Vec<u8> {
    let (tx, mut rx) = mpsc::channel(4);
    let writer = async {
        let mut writer = ExportWriter::new(Vec::new(), format);
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Columns(columns) => writer.begin(&columns).unwrap(),
                StreamEvent::Rows(rows) => writer.rows(&rows, dialect.as_ref()).unwrap(),
            }
        }
        writer.finish().unwrap()
    };
    let (sent, bytes) = tokio::join!(connection.stream_rows(sql, &[], tx), writer);
    sent.unwrap();
    bytes
}

async fn differences(admin: &mut Conn, a: &str, b: &str) -> (i64, i64) {
    let sql = format!(
        "SELECT (SELECT count(*) FROM ((SELECT * FROM {a} EXCEPT ALL SELECT * FROM {b}) UNION ALL (SELECT * FROM {b} EXCEPT ALL SELECT * FROM {a})) d), \
                (SELECT count(*) FROM {b})"
    );
    admin.query_first(sql).await.unwrap().unwrap()
}

async fn round_trip<I: Image>(container: &ContainerAsync<I>) {
    let port = container.get_host_port_ipv4(3306).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("source.csv");
    let url = format!("mysql://root@127.0.0.1:{port}/mysql");
    let mut root = Conn::new(Opts::from_url(&url).unwrap()).await.unwrap();
    root.query_drop("CREATE DATABASE shop; SET GLOBAL local_infile = 1")
        .await
        .unwrap();
    let opts = OptsBuilder::from_opts(
        Opts::from_url(&format!("mysql://root@127.0.0.1:{port}/shop")).unwrap(),
    )
    .local_infile_handler(Some(WhiteListFsHandler::new([file.clone()])));
    let mut admin = Conn::new(opts).await.unwrap();
    admin
        .query_drop(format!(
            "CREATE TABLE source ({TYPES}); CREATE TABLE from_csv ({TYPES}); CREATE TABLE from_insert ({TYPES}); {ROWS}"
        ))
        .await
        .unwrap();
    let config = ConnectionConfig {
        endpoint: Endpoint::Tcp {
            dial_host: "127.0.0.1".into(),
            dial_port: port,
            server_name: "localhost".into(),
        },
        database: Some("shop".into()),
        user: Some("root".into()),
        password: None,
        tls: TlsConfig::default(),
        read_only: false,
        options: Default::default(),
    };
    let connection = MysqlDriver.connect(&config).await.unwrap();
    let dialect = MysqlDriver.dialect();
    let select = "SELECT * FROM source ORDER BY id";

    // MySQL reads the unquoted word NULL as NULL; quoted fields keep their text.
    let options = CsvOptions {
        null: "NULL".into(),
        ..CsvOptions::default()
    };
    std::fs::write(
        &file,
        export(
            connection.as_ref(),
            Arc::clone(&dialect),
            select,
            ExportFormat::Csv(options),
        )
        .await,
    )
    .unwrap();
    admin
        .query_drop(format!(
            "LOAD DATA LOCAL INFILE '{}' INTO TABLE from_csv CHARACTER SET utf8mb4 \
             FIELDS TERMINATED BY ',' OPTIONALLY ENCLOSED BY '\"' ESCAPED BY '' LINES TERMINATED BY '\\r\\n' \
             IGNORE 1 LINES ({COLUMNS}) SET data = UNHEX(SUBSTRING(@data, 3))",
            file.display()
        ))
        .await
        .unwrap();
    let table = TableRef {
        namespace: NamespaceRef {
            catalog: None,
            schema: Some("shop".into()),
        },
        name: "from_insert".into(),
    };
    let inserts = export(
        connection.as_ref(),
        dialect,
        select,
        ExportFormat::Insert { table, batch: 3 },
    )
    .await;
    admin
        .query_drop(String::from_utf8(inserts).unwrap())
        .await
        .unwrap();

    assert_eq!(
        differences(&mut admin, "source", "from_csv").await,
        (0, 4),
        "CSV"
    );
    assert_eq!(
        differences(&mut admin, "source", "from_insert").await,
        (0, 4),
        "INSERT"
    );
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn exports_round_trip_on_mysql() {
    let container = Mysql::default().with_tag("8.4").start().await.unwrap();
    round_trip(&container).await;
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn exports_round_trip_on_mariadb() {
    let container = Mariadb::default().with_tag("11.4").start().await.unwrap();
    round_trip(&container).await;
}
