//! Conformance group "schema edit" (task 9.8): every kind of change is planned and applied on
//! the real database, then the table is described again and compared with what
//! `expected_table` predicted. Type names and default spellings differ per database, so the
//! comparison covers names, nullability, keys, indexes, foreign keys and checks.

use std::sync::Arc;

use super::{Failure, Harness, ensure, fail};
use crate::dialect::Dialect;
use crate::driver::Connection;
use crate::result::{ExecuteOptions, StatementOutcome};
use crate::schema::{
    Check, ForeignKey, Index, Key, NamespaceRef, ReferentialAction, TableInfo, TableRef,
};
use crate::schema_edit::{
    ColumnDefinition, SchemaChange, SchemaEdit, apply, check_current, expected_table,
};
use crate::value::Value;

const ROWS: i64 = 2;

struct Fixture {
    connection: Arc<dyn Connection>,
    dialect: Arc<dyn Dialect>,
    namespace: NamespaceRef,
}

impl Fixture {
    fn table(&self, name: &str) -> TableRef {
        TableRef {
            namespace: self.namespace.clone(),
            name: name.into(),
        }
    }

    fn qualified(&self, name: &str) -> String {
        self.dialect.qualified_name(&self.table(name))
    }

    async fn run(&self, check: &'static str, sql: &str) -> Result<StatementOutcome, Failure> {
        self.connection
            .execute(sql, &ExecuteOptions::default())
            .await
            .map_err(|e| fail(check, format!("{sql}: {e}")))
    }

    async fn describe(&self, name: &str) -> Result<TableInfo, Failure> {
        self.connection
            .describe_table(&self.table(name))
            .await
            .map_err(|e| fail("describe_table", format!("{name}: {e}")))
    }

    async fn rows(&self, name: &str) -> Result<i64, Failure> {
        let sql = format!("SELECT count(*) FROM {}", self.qualified(name));
        let StatementOutcome::Rows(result) = self.run("count rows", &sql).await? else {
            return Err(fail("count rows", "no result set"));
        };
        let count = result.rows.first().and_then(|r| r.first()).cloned();
        match count {
            Some(Value::Int(n)) => Ok(n),
            Some(Value::UInt(n)) => i64::try_from(n).map_err(|e| fail("count rows", e.to_string())),
            Some(Value::Decimal(text) | Value::Text(text)) => text
                .parse()
                .map_err(|_| fail("count rows", format!("not a count: {text}"))),
            other => Err(fail("count rows", format!("not a count: {other:?}"))),
        }
    }

    /// Plans and applies `changes` to `name`, then checks the table against the prediction and
    /// that no row was lost.
    async fn edit(
        &self,
        label: &'static str,
        name: &str,
        changes: impl FnOnce(&TableInfo) -> Vec<SchemaChange>,
    ) -> Result<TableInfo, Failure> {
        let original = self.describe(name).await?;
        let edit = SchemaEdit {
            changes: changes(&original),
            original,
        };
        let expected = expected_table(self.dialect.as_ref(), &edit)
            .map_err(|e| fail(label, format!("expected_table: {e}")))?;
        let plan = self
            .connection
            .plan_schema_edit(&edit)
            .await
            .map_err(|e| fail(label, format!("plan: {e}")))?;
        apply(self.connection.as_ref(), &plan)
            .await
            .map_err(|e| fail(label, format!("apply {:?}: {e}", plan.preview())))?;
        let actual = self.describe(&expected.table.name).await?;
        compare(label, &expected, &actual)?;
        let rows = self.rows(&expected.table.name).await?;
        ensure(
            rows == ROWS,
            label,
            format!("{rows} rows after the edit, expected {ROWS}"),
        )?;
        Ok(actual)
    }
}

/// A foreign key without its name, which databases invent when none is given.
#[derive(Debug, PartialEq)]
struct ForeignKeyShape {
    columns: Vec<String>,
    table: String,
    referenced: Vec<String>,
    on_delete: Option<ReferentialAction>,
}

/// What every database must agree on after an edit.
#[derive(Debug, PartialEq)]
struct Shape {
    name: String,
    columns: Vec<(String, bool)>,
    primary_key: Option<Vec<String>>,
    indexes: Vec<(String, Vec<String>, bool)>,
    uniques: Vec<Vec<String>>,
    foreign_keys: Vec<ForeignKeyShape>,
    checks: usize,
}

fn shape(info: &TableInfo) -> Shape {
    let fk_names: Vec<&str> = info
        .foreign_keys
        .iter()
        .filter_map(|fk| fk.name.as_deref())
        .collect();
    let mut indexes: Vec<_> = info
        .indexes
        .iter()
        // MySQL creates an index for each foreign key, named after it.
        .filter(|i| !fk_names.contains(&i.name.as_str()))
        .map(|i| (i.name.clone(), i.columns.clone(), i.unique))
        .collect();
    indexes.sort();
    let mut uniques: Vec<_> = info
        .unique_constraints
        .iter()
        .map(|k| k.columns.clone())
        .collect();
    uniques.sort();
    Shape {
        name: info.table.name.clone(),
        columns: info
            .columns
            .iter()
            .map(|c| (c.name.clone(), c.nullable))
            .collect(),
        primary_key: info.primary_key.as_ref().map(|k| k.columns.clone()),
        indexes,
        uniques,
        foreign_keys: info
            .foreign_keys
            .iter()
            .map(|fk| ForeignKeyShape {
                columns: fk.columns.clone(),
                table: fk.referenced_table.name.clone(),
                referenced: fk.referenced_columns.clone(),
                on_delete: fk.on_delete,
            })
            .collect(),
        checks: info.check_constraints.len(),
    }
}

fn compare(label: &'static str, expected: &TableInfo, actual: &TableInfo) -> Result<(), Failure> {
    let (expected, actual) = (shape(expected), shape(actual));
    ensure(
        expected == actual,
        label,
        format!("expected {expected:#?}\ngot {actual:#?}"),
    )
}

fn definition(info: &TableInfo, name: &str) -> ColumnDefinition {
    let column = info.column_named(name);
    ColumnDefinition {
        name: name.into(),
        type_name: column
            .as_ref()
            .map(|c| c.type_name.clone())
            .unwrap_or_default(),
        nullable: column.as_ref().is_some_and(|c| c.nullable),
        default: column.as_ref().and_then(|c| c.default.clone()),
        comment: column.and_then(|c| c.comment),
    }
}

fn alter(name: &str, column: ColumnDefinition) -> SchemaChange {
    SchemaChange::AlterColumn {
        name: name.into(),
        column,
        using: None,
    }
}

async fn setup(fixture: &Fixture) -> Result<(), Failure> {
    let parent = fixture.qualified("se_parent");
    let orders = fixture.qualified("se_orders");
    for sql in [
        format!("CREATE TABLE {parent} (id integer NOT NULL PRIMARY KEY)"),
        format!(
            "CREATE TABLE {orders} (id integer NOT NULL PRIMARY KEY, parent_id integer, \
             total integer, note varchar(100))"
        ),
        format!("INSERT INTO {parent} (id) VALUES (1), (2)"),
        format!(
            "INSERT INTO {orders} (id, parent_id, total, note) VALUES (1, 1, 10, 'a'), (2, 2, 20, 'b')"
        ),
    ] {
        fixture.run("schema edit setup", &sql).await?;
    }
    Ok(())
}

pub async fn schema_edit_group(harness: &dyn Harness) -> Result<(), Failure> {
    let config = harness.database(false).await;
    let driver = harness.driver();
    if !driver.info().capabilities.schema_editing {
        return Ok(());
    }
    let fixture = Fixture {
        connection: driver
            .connect(&config)
            .await
            .map_err(|e| fail("connect", e.to_string()))?,
        dialect: driver.dialect(),
        namespace: harness.fixture_namespace(&config),
    };
    setup(&fixture).await?;
    column_changes(&fixture).await?;
    key_changes(&fixture).await?;
    failing_edit(&fixture).await?;
    stale_edit(&fixture).await?;
    Ok(())
}

async fn column_changes(f: &Fixture) -> Result<(), Failure> {
    f.edit("add column", "se_orders", |_| {
        vec![SchemaChange::AddColumn {
            column: ColumnDefinition {
                name: "shipped".into(),
                type_name: "smallint".into(),
                nullable: true,
                default: Some("0".into()),
                comment: None,
            },
        }]
    })
    .await?;
    f.edit("rename column", "se_orders", |info| {
        vec![alter(
            "note",
            ColumnDefinition {
                name: "remark".into(),
                ..definition(info, "note")
            },
        )]
    })
    .await?;
    f.edit("set not null", "se_orders", |info| {
        vec![alter(
            "total",
            ColumnDefinition {
                nullable: false,
                ..definition(info, "total")
            },
        )]
    })
    .await?;
    f.edit("change type", "se_orders", |info| {
        vec![alter(
            "total",
            ColumnDefinition {
                type_name: "bigint".into(),
                ..definition(info, "total")
            },
        )]
    })
    .await?;
    f.edit("drop column", "se_orders", |_| {
        vec![SchemaChange::DropColumn {
            name: "shipped".into(),
        }]
    })
    .await?;
    Ok(())
}

async fn key_changes(f: &Fixture) -> Result<(), Failure> {
    let index = Index {
        name: "se_orders_remark_idx".into(),
        columns: vec!["remark".into()],
        unique: false,
        method: None,
        predicate: None,
    };
    let after = f
        .edit("add index", "se_orders", |_| {
            vec![SchemaChange::AddIndex {
                index: index.clone(),
            }]
        })
        .await?;
    let added = after.indexes.iter().find(|i| i.name == index.name).cloned();
    let added = added.ok_or_else(|| fail("add index", "the index is missing"))?;
    f.edit("drop index", "se_orders", |_| {
        vec![SchemaChange::DropIndex { index: added }]
    })
    .await?;

    let after = f
        .edit("add unique", "se_orders", |_| {
            vec![SchemaChange::AddUnique {
                key: Key {
                    name: Some("se_orders_pt_key".into()),
                    columns: vec!["parent_id".into(), "total".into()],
                },
            }]
        })
        .await?;
    f.edit("drop unique", "se_orders", |_| {
        vec![SchemaChange::DropUnique {
            key: after.unique_constraints[0].clone(),
        }]
    })
    .await?;

    let after = f
        .edit("add foreign key", "se_orders", |_| {
            vec![SchemaChange::AddForeignKey {
                foreign_key: ForeignKey {
                    name: Some("se_orders_parent_fk".into()),
                    columns: vec!["parent_id".into()],
                    referenced_table: f.table("se_parent"),
                    referenced_columns: vec!["id".into()],
                    on_update: None,
                    on_delete: Some(ReferentialAction::Cascade),
                },
            }]
        })
        .await?;
    f.edit("drop foreign key", "se_orders", |_| {
        vec![SchemaChange::DropForeignKey {
            foreign_key: after.foreign_keys[0].clone(),
        }]
    })
    .await?;

    let after = f
        .edit("add check", "se_orders", |_| {
            vec![SchemaChange::AddCheck {
                check: Check {
                    name: Some("se_orders_total_check".into()),
                    expression: "total >= 0".into(),
                },
            }]
        })
        .await?;
    f.edit("drop check", "se_orders", |_| {
        vec![SchemaChange::DropCheck {
            check: after.check_constraints[0].clone(),
        }]
    })
    .await?;

    f.edit("replace primary key", "se_orders", |info| {
        vec![
            SchemaChange::DropPrimaryKey,
            alter(
                "parent_id",
                ColumnDefinition {
                    nullable: false,
                    ..definition(info, "parent_id")
                },
            ),
            SchemaChange::AddPrimaryKey {
                key: Key {
                    name: None,
                    columns: vec!["id".into(), "parent_id".into()],
                },
            },
        ]
    })
    .await?;
    f.edit("rename table", "se_orders", |_| {
        vec![SchemaChange::RenameTable {
            name: "se_purchases".into(),
        }]
    })
    .await?;
    Ok(())
}

/// A check the rows break fails the edit: all of it is undone where the database can undo DDL;
/// otherwise the report says how much stayed applied.
async fn failing_edit(f: &Fixture) -> Result<(), Failure> {
    let original = f.describe("se_purchases").await?;
    let edit = SchemaEdit {
        changes: vec![
            SchemaChange::AddColumn {
                column: ColumnDefinition {
                    name: "extra".into(),
                    type_name: "integer".into(),
                    nullable: true,
                    default: None,
                    comment: None,
                },
            },
            SchemaChange::AddCheck {
                check: Check {
                    name: Some("se_never".into()),
                    expression: "total < 0".into(),
                },
            },
        ],
        original: original.clone(),
    };
    let plan = f
        .connection
        .plan_schema_edit(&edit)
        .await
        .map_err(|e| fail("failing edit", format!("plan: {e}")))?;
    let Err(failure) = apply(f.connection.as_ref(), &plan).await else {
        return Err(fail("failing edit", "a check the rows break was added"));
    };
    let after = f.describe("se_purchases").await?;
    if plan.transactional || plan.statements.len() == 1 {
        ensure(failure.applied == 0, "failing edit", format!("{failure:?}"))?;
        ensure(
            after == original,
            "failing edit",
            "the table changed although the edit failed",
        )?;
    } else {
        ensure(
            failure.statement == Some(failure.applied),
            "failing edit",
            format!("the report does not add up: {failure:?}"),
        )?;
    }
    Ok(())
}

async fn stale_edit(f: &Fixture) -> Result<(), Failure> {
    let original = f.describe("se_purchases").await?;
    let sql = format!(
        "ALTER TABLE {} ADD COLUMN stale integer",
        f.qualified("se_purchases")
    );
    f.run("stale edit", &sql).await?;
    let refused = check_current(f.connection.as_ref(), &original).await;
    ensure(
        refused.is_err(),
        "stale edit",
        "a stale description was accepted",
    )
}
