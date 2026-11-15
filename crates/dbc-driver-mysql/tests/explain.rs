//! `EXPLAIN` against real servers: MySQL's tree format and MariaDB's classic table.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dbc_core::driver::{ConnectionConfig, Driver, Endpoint, TlsConfig};
use dbc_core::plan::PlanNode;
use dbc_driver_mysql::MysqlDriver;
use mysql_async::prelude::Queryable;
use mysql_async::{Conn, Opts};
use testcontainers_modules::mariadb::Mariadb;
use testcontainers_modules::mysql::Mysql;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, Image, ImageExt};

fn relations(plan: &PlanNode) -> Vec<String> {
    let mut found: Vec<String> = plan.relation.iter().cloned().collect();
    for child in &plan.children {
        found.extend(relations(child));
    }
    found
}

async fn plan_of<I: Image>(container: &ContainerAsync<I>) -> PlanNode {
    let port = container.get_host_port_ipv4(3306).await.unwrap();
    let url = format!("mysql://root@127.0.0.1:{port}/mysql");
    let mut root = Conn::new(Opts::from_url(&url).unwrap()).await.unwrap();
    root.query_drop(
        "CREATE DATABASE shop; USE shop; \
         CREATE TABLE people (id INT PRIMARY KEY, name TEXT); \
         CREATE TABLE orders (id INT PRIMARY KEY, person INT, INDEX (person)); \
         INSERT INTO people VALUES (1, 'a'), (2, 'b');",
    )
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
    let plan = connection
        .explain("SELECT p.name FROM people p JOIN orders o ON o.person = p.id WHERE p.id > 0;")
        .await
        .unwrap();
    connection.explain("DELETE FROM people").await.unwrap();
    let left: (i64,) = root
        .query_first("SELECT count(*) FROM shop.people")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(left.0, 2, "EXPLAIN must not run the DELETE");
    plan
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn mysql_plans_come_from_the_tree_format() {
    let container = Mysql::default().with_tag("8.4").start().await.unwrap();

    let plan = plan_of(&container).await;

    assert!(plan.cost.is_some(), "{plan:?}");
    let found = relations(&plan);
    assert!(
        found.iter().any(|r| r == "p") && found.iter().any(|r| r == "o"),
        "{plan:?}"
    );
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn mariadb_plans_come_from_the_classic_table() {
    let container = Mariadb::default().with_tag("11.4").start().await.unwrap();

    let plan = plan_of(&container).await;

    assert_eq!(plan.operation, dbc_core::plan::QUERY);
    assert_eq!(relations(&plan).len(), 2, "{plan:?}");
}

#[tokio::test]
#[ignore = "needs Docker"]
async fn a_script_with_delimiter_commands_runs_statement_by_statement() {
    use dbc_core::result::{ExecuteOptions, StatementOutcome};

    let container = Mysql::default().with_tag("8.4").start().await.unwrap();
    let port = container.get_host_port_ipv4(3306).await.unwrap();
    let url = format!("mysql://root@127.0.0.1:{port}/mysql");
    let mut root = Conn::new(Opts::from_url(&url).unwrap()).await.unwrap();
    root.query_drop("CREATE DATABASE shop").await.unwrap();
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
    let script = "CREATE TABLE t (x INT);\n\
        DELIMITER $$\n\
        CREATE PROCEDURE fill(n INT)\n\
        BEGIN\n  DECLARE i INT DEFAULT 0;\n  WHILE i < n DO\n    INSERT INTO t VALUES (i);\n    SET i = i + 1;\n  END WHILE;\nEND $$\n\
        DELIMITER ;\n\
        CALL fill(3);\n\
        SELECT count(*) FROM t;";

    let rules = MysqlDriver.dialect().splitter_rules();
    let ranges = dbc_core::sql::split(script, rules);
    let mut last = None;
    for range in &ranges {
        let statement = &script[range.start..range.end];
        last = Some(
            connection
                .execute(statement, &ExecuteOptions::default())
                .await
                .unwrap(),
        );
    }

    assert_eq!(ranges.len(), 4);
    let Some(StatementOutcome::Rows(rows)) = last else {
        panic!("{last:?}")
    };
    assert_eq!(format!("{:?}", rows.rows[0][0]), "Int(3)");
}
