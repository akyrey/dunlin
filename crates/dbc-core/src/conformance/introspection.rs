//! Introspection conformance: the fixture schema as every driver must describe it.
//!
//! The expectations cover what is the same on every database: names, nullability, keys,
//! foreign keys and their actions, indexes, checks and `row_key`. Type names, defaults and
//! expression text differ per database and are covered by each driver's own snapshot tests.

use std::collections::BTreeSet;

use crate::driver::Connection;
use crate::error::DbErrorCode;
use crate::schema::{ForeignKey, NamespaceRef, ObjectKind, ReferentialAction, TableInfo, TableRef};

use super::{Failure, Harness, ensure, fail};

/// What a database can express or report, so the shared expectations skip the rest.
#[derive(Debug, Clone)]
pub struct IntrospectionSupport {
    /// Reports the names of foreign key, unique and check constraints (SQLite does not).
    pub constraint_names: bool,
    pub comments: bool,
    pub partial_indexes: bool,
    pub expression_indexes: bool,
    /// Fixture objects only this database has.
    pub extra_tables: Vec<&'static str>,
}

impl Default for IntrospectionSupport {
    fn default() -> Self {
        Self {
            constraint_names: true,
            comments: true,
            partial_indexes: true,
            expression_indexes: true,
            extra_tables: Vec::new(),
        }
    }
}

const MIXED: &str = "Mixed Case \"Quoted\"";

struct ExpectedColumn {
    name: &'static str,
    /// `None` skips the check (view columns).
    nullable: Option<bool>,
    generated: bool,
}

const fn col(name: &'static str, nullable: bool) -> ExpectedColumn {
    ExpectedColumn {
        name,
        nullable: Some(nullable),
        generated: false,
    }
}

struct ExpectedIndex {
    name: &'static str,
    /// `None` for an expression index: one key part that is not a column.
    columns: Option<&'static [&'static str]>,
    partial: bool,
}

struct ExpectedForeignKey {
    /// Checked only when the fixture names the constraint.
    name: Option<&'static str>,
    columns: &'static [&'static str],
    table: &'static str,
    referenced: &'static [&'static str],
    on_delete: Option<ReferentialAction>,
    on_update: Option<ReferentialAction>,
}

struct ExpectedTable {
    name: &'static str,
    kind: ObjectKind,
    comment: Option<&'static str>,
    columns: Vec<ExpectedColumn>,
    primary_key: Option<&'static [&'static str]>,
    /// (name when the fixture names it, columns)
    unique: Vec<(Option<&'static str>, &'static [&'static str])>,
    indexes: Vec<ExpectedIndex>,
    foreign_keys: Vec<ExpectedForeignKey>,
    /// Names of the expected checks (`None` = unnamed); `None` skips the check entirely.
    checks: Option<Vec<Option<&'static str>>>,
    row_key: Option<&'static [&'static str]>,
}

impl ExpectedTable {
    fn table(name: &'static str, columns: Vec<ExpectedColumn>) -> Self {
        Self {
            name,
            kind: ObjectKind::Table,
            comment: None,
            columns,
            primary_key: None,
            unique: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            checks: Some(Vec::new()),
            row_key: None,
        }
    }

    fn keyed(
        name: &'static str,
        key: &'static [&'static str],
        columns: Vec<ExpectedColumn>,
    ) -> Self {
        Self {
            primary_key: Some(key),
            row_key: Some(key),
            ..Self::table(name, columns)
        }
    }
}

fn fixture_tables() -> Vec<ExpectedTable> {
    let all_types = [
        "c_bool",
        "c_int",
        "c_bigint",
        "c_real",
        "c_decimal",
        "c_text",
        "c_blob",
        "c_json",
        "c_xml",
        "c_uuid",
        "c_date",
        "c_ts",
    ];
    let mut all_types_columns = vec![col("id", false)];
    all_types_columns.extend(all_types.into_iter().map(|name| col(name, true)));
    vec![
        ExpectedTable {
            unique: vec![(None, &["email"])],
            ..ExpectedTable::keyed(
                "customers",
                &["id"],
                vec![
                    col("id", false),
                    col("name", false),
                    col("email", true),
                    col("created_at", true),
                ],
            )
        },
        ExpectedTable {
            indexes: vec![ExpectedIndex {
                name: "orders_customer_idx",
                columns: Some(&["customer_id"]),
                partial: false,
            }],
            foreign_keys: vec![ExpectedForeignKey {
                name: None,
                columns: &["customer_id"],
                table: "customers",
                referenced: &["id"],
                on_delete: Some(ReferentialAction::Cascade),
                on_update: None,
            }],
            checks: Some(vec![None]),
            ..ExpectedTable::keyed(
                "orders",
                &["id"],
                vec![
                    col("id", false),
                    col("customer_id", false),
                    col("total", true),
                    col("placed_on", true),
                    col("paid", false),
                ],
            )
        },
        ExpectedTable {
            foreign_keys: vec![ExpectedForeignKey {
                name: None,
                columns: &["order_id"],
                table: "orders",
                referenced: &["id"],
                on_delete: None,
                on_update: None,
            }],
            ..ExpectedTable::keyed(
                "order_items",
                &["order_id", "line"],
                vec![
                    col("order_id", false),
                    col("line", false),
                    col("sku", false),
                    col("quantity", false),
                ],
            )
        },
        ExpectedTable {
            // MariaDB adds a json_valid() check to JSON columns.
            checks: None,
            ..ExpectedTable::keyed("all_types", &["id"], all_types_columns)
        },
        ExpectedTable::table(
            "event_log",
            vec![col("happened_at", true), col("message", true)],
        ),
        ExpectedTable {
            indexes: vec![ExpectedIndex {
                name: "Mixed \"idx\"",
                columns: Some(&["spaced name"]),
                partial: false,
            }],
            ..ExpectedTable::keyed(
                MIXED,
                &["select"],
                vec![
                    col("select", false),
                    col("spaced name", true),
                    col("naïve", true),
                ],
            )
        },
        ExpectedTable {
            kind: ObjectKind::View,
            checks: None,
            columns: ["id", "customer_id", "total"]
                .into_iter()
                .map(|name| ExpectedColumn {
                    name,
                    nullable: None,
                    generated: false,
                })
                .collect(),
            ..ExpectedTable::table("paid_orders", Vec::new())
        },
        employees(),
        ExpectedTable {
            indexes: vec![ExpectedIndex {
                name: "shipments_item_idx",
                columns: Some(&["order_id", "line"]),
                partial: false,
            }],
            foreign_keys: vec![ExpectedForeignKey {
                name: Some("shipments_item_fk"),
                columns: &["order_id", "line"],
                table: "order_items",
                referenced: &["order_id", "line"],
                on_delete: Some(ReferentialAction::Cascade),
                on_update: Some(ReferentialAction::Cascade),
            }],
            ..ExpectedTable::keyed(
                "shipments",
                &["id"],
                vec![col("id", false), col("order_id", false), col("line", false)],
            )
        },
        ExpectedTable {
            unique: vec![(None, &["code"])],
            row_key: Some(&["code"]),
            ..ExpectedTable::table("tags", vec![col("code", false), col("label", true)])
        },
    ]
}

fn employees() -> ExpectedTable {
    ExpectedTable {
        comment: Some("Staff, managed by other staff"),
        unique: vec![(Some("employees_name_key"), &["last_name", "first_name"])],
        indexes: vec![
            ExpectedIndex {
                name: "employees_manager_name_idx",
                columns: Some(&["manager_id", "last_name"]),
                partial: false,
            },
            ExpectedIndex {
                name: "employees_high_paid_idx",
                columns: Some(&["salary"]),
                partial: true,
            },
            ExpectedIndex {
                name: "employees_lower_last_idx",
                columns: None,
                partial: false,
            },
        ],
        foreign_keys: vec![ExpectedForeignKey {
            name: None,
            columns: &["manager_id"],
            table: "employees",
            referenced: &["id"],
            on_delete: Some(ReferentialAction::SetNull),
            on_update: None,
        }],
        checks: Some(vec![Some("salary_positive")]),
        ..ExpectedTable::keyed(
            "employees",
            &["id"],
            vec![
                col("id", false),
                col("manager_id", true),
                col("first_name", false),
                col("last_name", false),
                ExpectedColumn {
                    name: "full_name",
                    nullable: Some(true),
                    generated: true,
                },
                col("salary", true),
            ],
        )
    }
}

/// Namespaces, objects, and the full description and DDL of every fixture table.
pub async fn introspection_group(harness: &dyn Harness) -> Result<(), Failure> {
    let config = harness.database(false).await;
    let connection = harness
        .driver()
        .connect(&config)
        .await
        .map_err(|e| fail("connect", e.to_string()))?;
    let namespace = harness.fixture_namespace(&config);
    let support = harness.introspection_support();
    check_namespaces(connection.as_ref(), &namespace, harness.system_namespace()).await?;
    check_objects(connection.as_ref(), &namespace, &support).await?;
    for expected in fixture_tables() {
        let table = TableRef {
            namespace: namespace.clone(),
            name: expected.name.to_owned(),
        };
        let info = connection
            .describe_table(&table)
            .await
            .map_err(|e| fail("describe_table", format!("{}: {e}", expected.name)))?;
        check_table(&info, &expected, &namespace, &support)
            .map_err(|e| fail(e.check, format!("{}: {}", expected.name, e.message)))?;
        let ddl = connection
            .table_ddl(&table)
            .await
            .map_err(|e| fail("table_ddl", format!("{}: {e}", expected.name)))?;
        ensure(
            ddl.contains(expected.name) || ddl.contains(&expected.name.replace('"', "\"\"")),
            "table_ddl",
            format!("{}: DDL does not name the table: {ddl}", expected.name),
        )?;
    }
    check_missing(connection.as_ref(), &namespace).await?;
    connection
        .close()
        .await
        .map_err(|e| fail("close", e.to_string()))
}

async fn check_namespaces(
    connection: &dyn Connection,
    fixture: &NamespaceRef,
    system: Option<String>,
) -> Result<(), Failure> {
    let namespaces = connection
        .list_namespaces()
        .await
        .map_err(|e| fail("list_namespaces", e.to_string()))?;
    let found = namespaces.iter().find(|n| &n.namespace == fixture);
    ensure(
        found.is_some_and(|n| !n.system),
        "list_namespaces",
        format!("fixture namespace {fixture:?} missing or marked system: {namespaces:?}"),
    )?;
    if let Some(system) = system {
        let found = namespaces.iter().find(|n| n.name == system);
        ensure(
            found.is_some_and(|n| n.system),
            "list_namespaces",
            format!("{system} missing or not marked system"),
        )?;
    }
    Ok(())
}

async fn check_objects(
    connection: &dyn Connection,
    namespace: &NamespaceRef,
    support: &IntrospectionSupport,
) -> Result<(), Failure> {
    let objects = connection
        .list_objects(namespace)
        .await
        .map_err(|e| fail("list_objects", e.to_string()))?;
    let actual: BTreeSet<(String, ObjectKind)> =
        objects.iter().map(|o| (o.name.clone(), o.kind)).collect();
    let mut expected: BTreeSet<(String, ObjectKind)> = fixture_tables()
        .into_iter()
        .map(|t| (t.name.to_owned(), t.kind))
        .collect();
    expected.extend(
        support
            .extra_tables
            .iter()
            .map(|name| ((*name).to_owned(), ObjectKind::Table)),
    );
    ensure(
        actual == expected,
        "list_objects",
        format!("expected {expected:?}, got {actual:?}"),
    )?;
    let comment = objects
        .iter()
        .find(|o| o.name == "employees")
        .and_then(|o| o.comment.as_deref());
    let wanted = support.comments.then_some("Staff, managed by other staff");
    ensure(
        comment == wanted,
        "list_objects comment",
        format!("expected {wanted:?}, got {comment:?}"),
    )
}

async fn check_missing(
    connection: &dyn Connection,
    namespace: &NamespaceRef,
) -> Result<(), Failure> {
    let missing = TableRef {
        namespace: namespace.clone(),
        name: "no_such_table".to_owned(),
    };
    let code = connection
        .describe_table(&missing)
        .await
        .err()
        .map(|e| e.code);
    ensure(
        code == Some(DbErrorCode::NotFound),
        "describe missing table",
        format!("expected NotFound, got {code:?}"),
    )
}

fn names(columns: &[String]) -> Vec<&str> {
    columns.iter().map(String::as_str).collect()
}

fn check_table(
    info: &TableInfo,
    expected: &ExpectedTable,
    namespace: &NamespaceRef,
    support: &IntrospectionSupport,
) -> Result<(), Failure> {
    ensure(
        info.kind == expected.kind,
        "kind",
        format!("got {:?}", info.kind),
    )?;
    ensure(
        info.table.name == expected.name && &info.table.namespace == namespace,
        "table ref",
        format!("got {:?}", info.table),
    )?;
    let comment = info.comment.as_deref();
    let wanted = expected.comment.filter(|_| support.comments);
    ensure(
        comment == wanted,
        "comment",
        format!("expected {wanted:?}, got {comment:?}"),
    )?;
    check_columns(info, expected, support)?;
    let primary_key = info.primary_key.as_ref().map(|k| names(&k.columns));
    ensure(
        primary_key == expected.primary_key.map(<[&str]>::to_vec),
        "primary key",
        format!("got {primary_key:?}"),
    )?;
    let row_key = info.row_key.as_deref().map(names);
    ensure(
        row_key == expected.row_key.map(<[&str]>::to_vec),
        "row key",
        format!("got {row_key:?}"),
    )?;
    ensure(
        info.row_key == info.choose_row_key(),
        "row key",
        "row_key does not follow TableInfo::choose_row_key",
    )?;
    check_unique(info, expected, support)?;
    check_indexes(info, expected, support)?;
    check_foreign_keys(info, expected, namespace, support)?;
    check_checks(info, expected, support)
}

fn check_columns(
    info: &TableInfo,
    expected: &ExpectedTable,
    support: &IntrospectionSupport,
) -> Result<(), Failure> {
    let actual: Vec<&str> = info.columns.iter().map(|c| c.name.as_str()).collect();
    let wanted: Vec<&str> = expected.columns.iter().map(|c| c.name).collect();
    ensure(
        actual == wanted,
        "columns",
        format!("expected {wanted:?}, got {actual:?}"),
    )?;
    for (position, (column, want)) in info.columns.iter().zip(&expected.columns).enumerate() {
        ensure(
            column.ordinal as usize == position + 1,
            "column ordinal",
            format!("{}: got {}", column.name, column.ordinal),
        )?;
        ensure(
            want.nullable.is_none_or(|n| n == column.nullable),
            "column nullable",
            format!("{}: got {}", column.name, column.nullable),
        )?;
        ensure(
            column.generated == want.generated,
            "column generated",
            format!("{}: got {}", column.name, column.generated),
        )?;
        ensure(
            !column.generated || column.default.as_deref().is_some_and(|d| !d.is_empty()),
            "generated expression",
            format!("{}: no expression in default", column.name),
        )?;
    }
    if support.comments && expected.name == "employees" {
        let salary = info.columns.iter().find(|c| c.name == "salary");
        let comment = salary.and_then(|c| c.comment.as_deref());
        ensure(
            comment == Some("Yearly, before tax"),
            "column comment",
            format!("salary: got {comment:?}"),
        )?;
    }
    Ok(())
}

fn check_unique(
    info: &TableInfo,
    expected: &ExpectedTable,
    support: &IntrospectionSupport,
) -> Result<(), Failure> {
    let actual: BTreeSet<Vec<&str>> = info
        .unique_constraints
        .iter()
        .map(|k| names(&k.columns))
        .collect();
    let wanted: BTreeSet<Vec<&str>> = expected.unique.iter().map(|(_, c)| c.to_vec()).collect();
    ensure(
        actual == wanted,
        "unique constraints",
        format!("expected {wanted:?}, got {actual:?}"),
    )?;
    for (name, columns) in &expected.unique {
        let Some(name) = name.filter(|_| support.constraint_names) else {
            continue;
        };
        let found = info
            .unique_constraints
            .iter()
            .find(|k| names(&k.columns) == *columns);
        ensure(
            found.is_some_and(|k| k.name.as_deref() == Some(name)),
            "unique constraint name",
            format!("{name}: got {found:?}"),
        )?;
    }
    Ok(())
}

fn check_indexes(
    info: &TableInfo,
    expected: &ExpectedTable,
    support: &IntrospectionSupport,
) -> Result<(), Failure> {
    let wanted: Vec<&ExpectedIndex> = expected
        .indexes
        .iter()
        .filter(|i| {
            (support.partial_indexes || !i.partial)
                && (support.expression_indexes || i.columns.is_some())
        })
        .collect();
    let actual: BTreeSet<&str> = info.indexes.iter().map(|i| i.name.as_str()).collect();
    let wanted_names: BTreeSet<&str> = wanted.iter().map(|i| i.name).collect();
    ensure(
        actual == wanted_names,
        "indexes",
        format!("expected {wanted_names:?}, got {actual:?} (constraint indexes are not listed)"),
    )?;
    for want in wanted {
        let Some(index) = info.indexes.iter().find(|i| i.name == want.name) else {
            continue;
        };
        let columns = names(&index.columns);
        let columns_ok = match want.columns {
            Some(expected) => columns == expected,
            None => columns.len() == 1 && info.columns.iter().all(|c| c.name != columns[0]),
        };
        ensure(
            columns_ok,
            "index columns",
            format!("{}: got {columns:?}", want.name),
        )?;
        ensure(
            !index.unique,
            "index unique",
            format!("{}: reported unique", want.name),
        )?;
        ensure(
            index.predicate.as_deref().is_some_and(|p| !p.is_empty()) == want.partial,
            "index predicate",
            format!("{}: got {:?}", want.name, index.predicate),
        )?;
    }
    Ok(())
}

fn check_foreign_keys(
    info: &TableInfo,
    expected: &ExpectedTable,
    namespace: &NamespaceRef,
    support: &IntrospectionSupport,
) -> Result<(), Failure> {
    ensure(
        info.foreign_keys.len() == expected.foreign_keys.len(),
        "foreign keys",
        format!(
            "expected {}, got {:?}",
            expected.foreign_keys.len(),
            info.foreign_keys
        ),
    )?;
    for want in &expected.foreign_keys {
        let found = info
            .foreign_keys
            .iter()
            .find(|fk| names(&fk.columns) == want.columns);
        let Some(fk) = found else {
            return Err(fail("foreign keys", format!("none on {:?}", want.columns)));
        };
        check_foreign_key(fk, want, namespace, support)?;
    }
    Ok(())
}

fn check_foreign_key(
    fk: &ForeignKey,
    want: &ExpectedForeignKey,
    namespace: &NamespaceRef,
    support: &IntrospectionSupport,
) -> Result<(), Failure> {
    let referenced = TableRef {
        namespace: namespace.clone(),
        name: want.table.to_owned(),
    };
    ensure(
        fk.referenced_table == referenced && names(&fk.referenced_columns) == want.referenced,
        "foreign key target",
        format!(
            "{:?}: got {:?} {:?}",
            want.columns, fk.referenced_table, fk.referenced_columns
        ),
    )?;
    ensure(
        fk.on_delete == want.on_delete && fk.on_update == want.on_update,
        "foreign key actions",
        format!(
            "{:?}: got delete {:?}, update {:?}",
            want.columns, fk.on_delete, fk.on_update
        ),
    )?;
    if let Some(name) = want.name.filter(|_| support.constraint_names) {
        ensure(
            fk.name.as_deref() == Some(name),
            "foreign key name",
            format!("expected {name}, got {:?}", fk.name),
        )?;
    }
    Ok(())
}

fn check_checks(
    info: &TableInfo,
    expected: &ExpectedTable,
    support: &IntrospectionSupport,
) -> Result<(), Failure> {
    let Some(wanted) = &expected.checks else {
        return Ok(());
    };
    ensure(
        info.check_constraints.len() == wanted.len(),
        "check constraints",
        format!(
            "expected {}, got {:?}",
            wanted.len(),
            info.check_constraints
        ),
    )?;
    ensure(
        info.check_constraints
            .iter()
            .all(|c| !c.expression.is_empty()),
        "check expression",
        "empty expression",
    )?;
    for name in wanted.iter().flatten().filter(|_| support.constraint_names) {
        ensure(
            info.check_constraints
                .iter()
                .any(|c| c.name.as_deref() == Some(*name)),
            "check constraint name",
            format!("{name} missing: {:?}", info.check_constraints),
        )?;
    }
    Ok(())
}
