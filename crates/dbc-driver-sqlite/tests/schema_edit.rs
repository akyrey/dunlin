//! Schema edits applied to real SQLite databases (task 9.7): native changes, the table rebuild,
//! and what must survive it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::sync::Arc;

use dbc_core::driver::{Connection, Driver};
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_core::schema::{
    Check, ForeignKey, Index, Key, NamespaceRef, ReferentialAction, TableInfo, TableRef,
};
use dbc_core::schema_edit::{
    ColumnDefinition, SchemaApplyFailure, SchemaChange, SchemaEdit, SchemaPlan, apply,
};
use dbc_core::{DbErrorCode, Value};
use dbc_driver_sqlite::SqliteDriver;

use common::{TempDatabases, creating};

const SCHEMA: &str = "
CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
CREATE TABLE orders (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  customer_id INTEGER REFERENCES customers(id) ON DELETE CASCADE,
  total INTEGER CHECK (total >= 0),
  note TEXT COLLATE NOCASE DEFAULT 'none',
  code TEXT UNIQUE,
  doubled INTEGER GENERATED ALWAYS AS (total * 2) VIRTUAL
);
CREATE INDEX orders_total_idx ON orders (total);
CREATE TABLE lines (
  order_id INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  line INTEGER NOT NULL,
  PRIMARY KEY (order_id, line)
) WITHOUT ROWID;
CREATE TABLE audit (what TEXT);
CREATE TRIGGER orders_audit AFTER INSERT ON orders BEGIN
  INSERT INTO audit (what) VALUES ('order ' || NEW.note);
END;
INSERT INTO customers (id, name) VALUES (1, 'Ada'), (2, 'Bo');
INSERT INTO orders (customer_id, total, note, code) VALUES (1, 10, 'a', 'x'), (2, 20, NULL, 'y');
DELETE FROM orders WHERE id = 2;
INSERT INTO orders (customer_id, total, note, code) VALUES (2, 30, 'c', 'z');
INSERT INTO lines (order_id, line) VALUES (1, 1), (1, 2), (3, 1);
";

struct Db {
    _databases: TempDatabases,
    connection: Arc<dyn Connection>,
}

async fn open() -> Db {
    let databases = TempDatabases::default();
    let path = databases.dir().join("edit.db");
    let connection = SqliteDriver.connect(&creating(&path)).await.unwrap();
    for statement in SCHEMA.split(";\n").filter(|s| !s.trim().is_empty()) {
        let statement = if statement.trim_start().starts_with("CREATE TRIGGER") {
            format!("{statement};\nEND")
        } else {
            statement.to_owned()
        };
        if statement.trim() == "END" {
            continue;
        }
        connection
            .execute(&statement, &ExecuteOptions::default())
            .await
            .unwrap_or_else(|e| panic!("{statement}: {e}"));
    }
    Db {
        _databases: databases,
        connection,
    }
}

fn table(name: &str) -> TableRef {
    TableRef {
        namespace: NamespaceRef {
            catalog: None,
            schema: Some("main".into()),
        },
        name: name.into(),
    }
}

impl Db {
    async fn describe(&self, name: &str) -> TableInfo {
        self.connection.describe_table(&table(name)).await.unwrap()
    }

    async fn plan(&self, changes: Vec<SchemaChange>) -> SchemaPlan {
        let original = self.describe("orders").await;
        self.connection
            .plan_schema_edit(&SchemaEdit { original, changes })
            .await
            .unwrap()
    }

    async fn apply(&self, changes: Vec<SchemaChange>) -> Result<SchemaPlan, SchemaApplyFailure> {
        let plan = self.plan(changes).await;
        apply(self.connection.as_ref(), &plan).await.map(|_| plan)
    }

    async fn rows(&self, sql: &str) -> Vec<Vec<Value>> {
        match self
            .connection
            .execute(sql, &ExecuteOptions::default())
            .await
            .unwrap()
        {
            StatementOutcome::Rows(result) => result.rows,
            other => panic!("expected rows, got {other:?}"),
        }
    }

    async fn one(&self, sql: &str) -> Value {
        self.rows(sql).await.remove(0).remove(0)
    }

    async fn stored_sql(&self, name: &str) -> String {
        match self
            .one(&format!(
                "SELECT sql FROM sqlite_master WHERE name = '{name}'"
            ))
            .await
        {
            Value::Text(text) => text,
            other => panic!("{other:?}"),
        }
    }
}

fn column_names(info: &TableInfo) -> Vec<&str> {
    info.columns.iter().map(|c| c.name.as_str()).collect()
}

fn definition(
    name: &str,
    type_name: &str,
    nullable: bool,
    default: Option<&str>,
) -> ColumnDefinition {
    ColumnDefinition {
        name: name.into(),
        type_name: type_name.into(),
        nullable,
        default: default.map(str::to_owned),
        comment: None,
    }
}

fn alter(name: &str, column: ColumnDefinition) -> SchemaChange {
    SchemaChange::AlterColumn {
        name: name.into(),
        column,
        using: None,
    }
}

#[tokio::test]
async fn simple_changes_run_natively() {
    let db = open().await;
    let plan = db
        .apply(vec![
            SchemaChange::AddColumn {
                column: definition("shipped", "INTEGER", true, Some("0")),
            },
            alter("note", definition("remark", "TEXT", true, Some("'none'"))),
            SchemaChange::AddIndex {
                index: Index {
                    name: "orders_lower_remark".into(),
                    columns: vec!["lower(remark)".into()],
                    unique: false,
                    method: None,
                    predicate: Some("remark IS NOT NULL".into()),
                },
            },
            SchemaChange::DropIndex {
                index: Index {
                    name: "orders_total_idx".into(),
                    columns: vec!["total".into()],
                    unique: false,
                    method: None,
                    predicate: None,
                },
            },
        ])
        .await
        .unwrap();
    assert!(plan.before.is_empty(), "no rebuild: {plan:?}");
    assert_eq!(plan.statements.len(), 4);
    let info = db.describe("orders").await;
    assert_eq!(
        column_names(&info),
        [
            "id",
            "customer_id",
            "total",
            "remark",
            "code",
            "doubled",
            "shipped"
        ]
    );
    let indexes: Vec<&str> = info.indexes.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(indexes, ["orders_lower_remark"]);
    // The trigger follows the rename.
    let trigger = db.stored_sql("orders_audit").await;
    assert!(trigger.contains("NEW.\"remark\""), "{trigger}");
}

#[tokio::test]
async fn a_type_change_rebuilds_and_keeps_everything_else() {
    let db = open().await;
    let before_audit = db.rows("SELECT count(*) FROM audit").await;
    let plan = db
        .apply(vec![alter(
            "total",
            definition("total", "REAL", false, Some("0")),
        )])
        .await
        .unwrap();
    assert!(plan.destructive);
    assert_eq!(
        plan.before,
        [
            "PRAGMA foreign_keys = OFF",
            "PRAGMA legacy_alter_table = ON"
        ]
    );
    assert_eq!(
        plan.after,
        [
            "PRAGMA foreign_keys = ON",
            "PRAGMA legacy_alter_table = OFF"
        ]
    );

    let info = db.describe("orders").await;
    let total = info.column_named("total").unwrap();
    assert_eq!(total.type_name, "REAL");
    assert!(!total.nullable);
    assert_eq!(total.default.as_deref(), Some("0"));

    let sql = db.stored_sql("orders").await;
    for kept in [
        "AUTOINCREMENT",
        "COLLATE NOCASE",
        "ON DELETE CASCADE",
        "CHECK (total >= 0)",
        "GENERATED ALWAYS AS (total * 2) VIRTUAL",
        "code TEXT UNIQUE",
    ] {
        assert!(sql.contains(kept), "{kept} lost from {sql}");
    }
    assert!(!sql.contains("_dunlin_rebuild_"), "{sql}");
    // Rows, the index, the trigger, the counter and the children all survive.
    assert_eq!(
        db.rows("SELECT id, total, doubled FROM orders ORDER BY id")
            .await,
        [
            [Value::Int(1), Value::Float(10.0), Value::Int(20)],
            [Value::Int(3), Value::Float(30.0), Value::Int(60)]
        ]
    );
    assert!(
        db.stored_sql("orders_total_idx")
            .await
            .contains("ON orders")
    );
    assert_eq!(db.one("SELECT count(*) FROM lines").await, Value::Int(3));
    assert_eq!(
        db.one("SELECT seq FROM sqlite_sequence WHERE name = 'orders'")
            .await,
        Value::Int(3)
    );
    db.rows("INSERT INTO orders (customer_id, total) VALUES (1, 5) RETURNING id")
        .await;
    assert_eq!(db.one("SELECT max(id) FROM orders").await, Value::Int(4));
    let after_audit = db.rows("SELECT count(*) FROM audit").await;
    assert_ne!(before_audit, after_audit, "the trigger still fires");
    assert_eq!(db.one("PRAGMA foreign_keys").await, Value::Int(1));
    assert_eq!(db.one("PRAGMA legacy_alter_table").await, Value::Int(0));
}

#[tokio::test]
async fn constraints_are_dropped_and_added_by_rebuilding() {
    let db = open().await;
    let info = db.describe("orders").await;
    db.apply(vec![
        SchemaChange::DropForeignKey {
            foreign_key: info.foreign_keys[0].clone(),
        },
        SchemaChange::DropCheck {
            check: info.check_constraints[0].clone(),
        },
        SchemaChange::DropUnique {
            key: info.unique_constraints[0].clone(),
        },
        SchemaChange::AddUnique {
            key: Key {
                name: Some("orders_code_note".into()),
                columns: vec!["code".into(), "note".into()],
            },
        },
        SchemaChange::AddCheck {
            check: Check {
                name: Some("positive".into()),
                expression: "total > 0".into(),
            },
        },
        SchemaChange::AddForeignKey {
            foreign_key: ForeignKey {
                name: Some("orders_customer".into()),
                columns: vec!["customer_id".into()],
                referenced_table: table("customers"),
                referenced_columns: vec!["id".into()],
                on_update: None,
                on_delete: Some(ReferentialAction::SetNull),
            },
        },
    ])
    .await
    .unwrap();
    let info = db.describe("orders").await;
    assert_eq!(info.foreign_keys.len(), 1);
    assert_eq!(
        info.foreign_keys[0].on_delete,
        Some(ReferentialAction::SetNull)
    );
    let checks: Vec<&str> = info
        .check_constraints
        .iter()
        .map(|c| c.expression.as_str())
        .collect();
    assert_eq!(checks, ["total > 0"]);
    assert_eq!(info.unique_constraints[0].columns, ["code", "note"]);
    let sql = db.stored_sql("orders").await;
    assert!(
        sql.contains("code TEXT,") || sql.contains("code TEXT\n"),
        "{sql}"
    );
}

#[tokio::test]
async fn the_primary_key_can_be_replaced() {
    let db = open().await;
    db.apply(vec![
        SchemaChange::DropPrimaryKey,
        SchemaChange::AddPrimaryKey {
            key: Key {
                name: None,
                columns: vec!["id".into(), "customer_id".into()],
            },
        },
    ])
    .await
    .unwrap();
    let info = db.describe("orders").await;
    assert_eq!(info.primary_key.unwrap().columns, ["id", "customer_id"]);
    assert_eq!(db.one("SELECT count(*) FROM orders").await, Value::Int(2));
}

#[tokio::test]
async fn rows_that_break_a_new_foreign_key_roll_everything_back() {
    let db = open().await;
    db.rows("INSERT INTO audit (what) VALUES ('99') RETURNING what")
        .await;
    let original = db.describe("audit").await;
    let edit = SchemaEdit {
        original: original.clone(),
        changes: vec![SchemaChange::AddForeignKey {
            foreign_key: ForeignKey {
                name: None,
                columns: vec!["what".into()],
                referenced_table: table("customers"),
                referenced_columns: vec!["id".into()],
                on_update: None,
                on_delete: None,
            },
        }],
    };
    let plan = db.connection.plan_schema_edit(&edit).await.unwrap();
    assert_eq!(plan.checks.len(), 1);
    let failure = apply(db.connection.as_ref(), &plan).await.unwrap_err();
    assert_eq!(failure.error.code, DbErrorCode::Constraint, "{failure:?}");
    assert_eq!(db.describe("audit").await, original);
    assert_eq!(db.one("PRAGMA foreign_keys").await, Value::Int(1));
}

#[tokio::test]
async fn a_failing_rebuild_leaves_the_table_as_it_was() {
    let db = open().await;
    db.rows("UPDATE orders SET note = NULL WHERE id = 1 RETURNING id")
        .await;
    let original = db.describe("orders").await;
    let failure = db
        .apply(vec![alter(
            "note",
            definition("note", "TEXT", false, Some("'none'")),
        )])
        .await
        .unwrap_err();
    assert!(failure.error.message.contains("NOT NULL"), "{failure:?}");
    assert_eq!(db.describe("orders").await, original);
    assert_eq!(db.one("SELECT count(*) FROM orders").await, Value::Int(2));
    assert_eq!(db.one("SELECT count(*) FROM lines").await, Value::Int(3));
    assert_eq!(db.one("PRAGMA foreign_keys").await, Value::Int(1));
}

#[tokio::test]
async fn renames_and_rebuilds_combine() {
    let db = open().await;
    db.apply(vec![
        alter("note", definition("remark", "TEXT", true, Some("'none'"))),
        alter("total", definition("total", "INTEGER", false, Some("0"))),
        SchemaChange::AddIndex {
            index: Index {
                name: "orders_remark_idx".into(),
                columns: vec!["remark".into()],
                unique: false,
                method: None,
                predicate: None,
            },
        },
        SchemaChange::RenameTable {
            name: "purchases".into(),
        },
    ])
    .await
    .unwrap();
    let info = db.describe("purchases").await;
    assert!(info.column_named("remark").is_some());
    assert!(!info.column_named("total").unwrap().nullable);
    let indexes: Vec<&str> = info.indexes.iter().map(|i| i.name.as_str()).collect();
    assert!(indexes.contains(&"orders_remark_idx"), "{indexes:?}");
    assert!(
        db.stored_sql("orders_audit")
            .await
            .contains("NEW.\"remark\"")
    );
    assert!(db.stored_sql("lines").await.contains("purchases"));
}

#[tokio::test]
async fn comments_are_refused() {
    let db = open().await;
    let original = db.describe("orders").await;
    let error = db
        .connection
        .plan_schema_edit(&SchemaEdit {
            original,
            changes: vec![SchemaChange::SetComment {
                comment: Some("x".into()),
            }],
        })
        .await
        .unwrap_err();
    assert!(error.message.contains("comments"), "{}", error.message);
}

#[tokio::test]
async fn table_options_survive_a_rebuild() {
    let db = open().await;
    let original = db.describe("lines").await;
    let plan = db
        .connection
        .plan_schema_edit(&SchemaEdit {
            original,
            changes: vec![SchemaChange::AddCheck {
                check: Check {
                    name: None,
                    expression: "line > 0".into(),
                },
            }],
        })
        .await
        .unwrap();
    apply(db.connection.as_ref(), &plan).await.unwrap();
    let sql = db.stored_sql("lines").await;
    assert!(sql.ends_with(") WITHOUT ROWID"), "{sql}");
    assert!(sql.contains("CHECK (line > 0)"), "{sql}");
    assert_eq!(db.one("SELECT count(*) FROM lines").await, Value::Int(3));
}
