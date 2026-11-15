#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::sync::Arc;

use dbc_core::DbErrorCode;
use dbc_core::driver::{Connection, Driver};
use dbc_core::result::ExecuteOptions;
use dbc_core::schema::{NamespaceRef, TableRef};
use dbc_driver_sqlite::SqliteDriver;

use common::{TempDatabases, config};

const FIXTURE_TABLES: [&str; 11] = [
    "customers",
    "orders",
    "order_items",
    "all_types",
    "event_log",
    "Mixed Case \"Quoted\"",
    "paid_orders",
    "employees",
    "shipments",
    "tags",
    "settings",
];

async fn open(databases: &TempDatabases) -> Arc<dyn Connection> {
    SqliteDriver
        .connect(&config(databases.fixture(), false))
        .await
        .unwrap()
}

fn in_schema(schema: &str, name: &str) -> TableRef {
    TableRef {
        namespace: NamespaceRef {
            catalog: None,
            schema: Some(schema.into()),
        },
        name: name.into(),
    }
}

#[tokio::test]
async fn describes_every_fixture_table() {
    let databases = TempDatabases::default();
    let connection = open(&databases).await;

    for name in FIXTURE_TABLES {
        let info = connection
            .describe_table(&in_schema("main", name))
            .await
            .unwrap();

        insta::assert_json_snapshot!(format!("describe_{}", name.replace([' ', '"'], "_")), info);
    }
}

#[tokio::test]
async fn ddl_is_the_stored_text_with_indexes() {
    let databases = TempDatabases::default();
    let connection = open(&databases).await;

    let ddl = connection
        .table_ddl(&in_schema("main", "employees"))
        .await
        .unwrap();

    insta::assert_snapshot!(ddl);
}

#[tokio::test]
async fn without_rowid_keys_are_not_null() {
    let databases = TempDatabases::default();
    let connection = open(&databases).await;

    let info = connection
        .describe_table(&in_schema("main", "settings"))
        .await
        .unwrap();

    assert!(!info.columns[0].nullable);
    assert_eq!(info.row_key, Some(vec!["key".to_owned()]));
}

#[tokio::test]
async fn attached_databases_are_namespaces() {
    let databases = TempDatabases::default();
    let connection = open(&databases).await;
    let other = databases.fixture();
    let attach = format!("ATTACH DATABASE '{}' AS \"other db\"", other.display());
    connection
        .execute(&attach, &ExecuteOptions::default())
        .await
        .unwrap();

    let namespaces = connection.list_namespaces().await.unwrap();
    let names: Vec<(&str, bool)> = namespaces
        .iter()
        .map(|n| (n.name.as_str(), n.system))
        .collect();
    let objects = connection
        .list_objects(&in_schema("other db", "").namespace)
        .await
        .unwrap();
    let info = connection
        .describe_table(&in_schema("other db", "employees"))
        .await
        .unwrap();
    let ddl = connection
        .table_ddl(&in_schema("other db", "tags"))
        .await
        .unwrap();

    assert_eq!(names, vec![("main", false), ("other db", false)]);
    assert_eq!(objects.len(), FIXTURE_TABLES.len());
    assert_eq!(
        info.foreign_keys[0].referenced_table,
        in_schema("other db", "employees")
    );
    assert!(ddl.starts_with("CREATE TABLE tags"));
}

#[tokio::test]
async fn unknown_tables_and_schemas_are_not_found() {
    let databases = TempDatabases::default();
    let connection = open(&databases).await;

    let missing_table = connection
        .table_ddl(&in_schema("main", "nope"))
        .await
        .unwrap_err();
    let missing_schema = connection
        .describe_table(&in_schema("nope", "employees"))
        .await
        .unwrap_err();
    let objects = connection
        .list_objects(&in_schema("nope", "").namespace)
        .await
        .unwrap();

    assert_eq!(missing_table.code, DbErrorCode::NotFound);
    assert_eq!(missing_schema.code, DbErrorCode::NotFound);
    assert!(objects.is_empty());
}

#[tokio::test]
async fn foreign_keys_without_columns_reference_the_primary_key() {
    let databases = TempDatabases::default();
    let connection = open(&databases).await;
    connection
        .execute(
            "CREATE TABLE notes (id INTEGER PRIMARY KEY, tag TEXT REFERENCES tags, owner INTEGER REFERENCES customers)",
            &ExecuteOptions::default(),
        )
        .await
        .unwrap();

    let info = connection
        .describe_table(&in_schema("main", "notes"))
        .await
        .unwrap();
    let referenced: Vec<Vec<String>> = info
        .foreign_keys
        .iter()
        .map(|fk| fk.referenced_columns.clone())
        .collect();

    // `tags` has no primary key, so nothing can be filled in for it.
    assert_eq!(
        referenced,
        vec![Vec::<String>::new(), vec!["id".to_owned()]]
    );
}

#[tokio::test]
async fn explain_returns_the_query_plan_as_a_tree_without_running_the_statement() {
    let databases = TempDatabases::default();
    let connection = open(&databases).await;

    let plan = connection
        .explain("SELECT c.name FROM customers c JOIN orders o ON o.customer_id = c.id ORDER BY 1;")
        .await
        .unwrap();
    let deleted = connection.explain("DELETE FROM orders").await.unwrap();
    let count = connection
        .execute("SELECT count(*) FROM orders", &ExecuteOptions::default())
        .await
        .unwrap();

    let mut relations = Vec::new();
    let mut stack = vec![&plan];
    while let Some(node) = stack.pop() {
        relations.extend(node.relation.clone());
        stack.extend(&node.children);
    }
    assert!(
        relations.contains(&"c".to_owned()) || relations.contains(&"customers".to_owned()),
        "{plan:?}"
    );
    assert!(!deleted.operation.is_empty());
    let dbc_core::result::StatementOutcome::Rows(rows) = count else {
        panic!("{count:?}")
    };
    assert_ne!(
        rows.rows[0][0],
        dbc_core::Value::Int(0),
        "EXPLAIN must not run the DELETE"
    );
}

#[tokio::test]
async fn tables_without_a_key_are_addressed_by_their_rowid() {
    let databases = TempDatabases::default();
    let connection = open(&databases).await;
    connection
        .execute(
            "CREATE TABLE shadow (rowid TEXT, value TEXT)",
            &ExecuteOptions::default(),
        )
        .await
        .unwrap();

    let events = connection
        .describe_table(&in_schema("main", "event_log"))
        .await
        .unwrap();
    let settings = connection
        .describe_table(&in_schema("main", "settings"))
        .await
        .unwrap();
    let shadow = connection
        .describe_table(&in_schema("main", "shadow"))
        .await
        .unwrap();
    let orders = connection
        .describe_table(&in_schema("main", "orders"))
        .await
        .unwrap();

    assert_eq!(events.row_id.as_deref(), Some("rowid"));
    assert_eq!(events.row_key, Some(vec!["rowid".to_owned()]));
    assert_eq!(settings.row_id, None, "WITHOUT ROWID tables have no rowid");
    assert_eq!(shadow.row_id.as_deref(), Some("_rowid_"));
    assert!(!orders.uses_row_id(), "a real key wins");
}
