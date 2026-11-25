//! Schema edits every SQL driver snapshots (task 9.4, 9.5): each kind of change on its own,
//! the combinations planners treat specially, and awkward identifiers.

use crate::result::TypeCategory;
use crate::schema::{
    Check, Column, ForeignKey, Index, Key, NamespaceRef, ObjectKind, ReferentialAction, TableInfo,
    TableRef,
};
use crate::schema_edit::{ColumnDefinition, SchemaChange, SchemaEdit};

fn table(namespace: &NamespaceRef, name: &str) -> TableRef {
    TableRef {
        namespace: namespace.clone(),
        name: name.into(),
    }
}

fn column(name: &str, ordinal: u32, type_name: &str, nullable: bool) -> Column {
    Column {
        name: name.into(),
        type_name: type_name.into(),
        category: TypeCategory::Other,
        nullable,
        default: None,
        comment: None,
        ordinal,
        generated: false,
    }
}

fn names(columns: &[&str]) -> Vec<String> {
    columns.iter().map(|c| (*c).to_owned()).collect()
}

fn key(name: &str, columns: &[&str]) -> Key {
    Key {
        name: Some(name.into()),
        columns: names(columns),
    }
}

/// `orders`: `id` primary key, `customer_id` referencing `customers`, `total` with an index
/// and a check, `note` with a default and a unique key, and a column whose name needs quoting.
pub fn schema_edit_table(namespace: &NamespaceRef) -> TableInfo {
    TableInfo {
        table: table(namespace, "orders"),
        kind: ObjectKind::Table,
        comment: None,
        columns: vec![
            column("id", 1, "integer", false),
            column("customer_id", 2, "integer", true),
            column("total", 3, "integer", true),
            Column {
                default: Some("'none'".into()),
                ..column("note", 4, "varchar(100)", true)
            },
            column("odd \"name\"", 5, "integer", true),
        ],
        primary_key: Some(key("orders_pkey", &["id"])),
        indexes: vec![Index {
            name: "orders_total_idx".into(),
            columns: names(&["total"]),
            unique: false,
            method: None,
            predicate: None,
        }],
        foreign_keys: vec![existing_foreign_key(namespace)],
        unique_constraints: vec![key("orders_note_key", &["note"])],
        check_constraints: vec![Check {
            name: Some("orders_total_check".into()),
            expression: "total >= 0".into(),
        }],
        row_key: Some(names(&["id"])),
        row_id: None,
    }
}

fn existing_foreign_key(namespace: &NamespaceRef) -> ForeignKey {
    ForeignKey {
        name: Some("orders_customer_fk".into()),
        columns: names(&["customer_id"]),
        referenced_table: table(namespace, "customers"),
        referenced_columns: names(&["id"]),
        on_update: None,
        on_delete: None,
    }
}

fn definition(name: &str, type_name: &str, nullable: bool) -> ColumnDefinition {
    ColumnDefinition {
        name: name.into(),
        type_name: type_name.into(),
        nullable,
        default: None,
        comment: None,
    }
}

/// The column as it is in [`schema_edit_table`], to alter one attribute of it.
fn as_is(info: &TableInfo, name: &str) -> ColumnDefinition {
    let found = info.column_named(name);
    ColumnDefinition {
        name: name.into(),
        type_name: found
            .as_ref()
            .map(|c| c.type_name.clone())
            .unwrap_or_default(),
        nullable: found.as_ref().is_some_and(|c| c.nullable),
        default: found.as_ref().and_then(|c| c.default.clone()),
        comment: found.and_then(|c| c.comment),
    }
}

fn alter(name: &str, column: ColumnDefinition, using: Option<&str>) -> SchemaChange {
    SchemaChange::AlterColumn {
        name: name.into(),
        column,
        using: using.map(str::to_owned),
    }
}

/// Labelled edits of [`schema_edit_table`].
pub fn schema_edit_cases(namespace: &NamespaceRef) -> Vec<(&'static str, SchemaEdit)> {
    let info = schema_edit_table(namespace);
    let edit = |changes: Vec<SchemaChange>| SchemaEdit {
        original: info.clone(),
        changes,
    };
    let column_changes = vec![
        (
            "add column",
            edit(vec![SchemaChange::AddColumn {
                column: ColumnDefinition {
                    default: Some("0".into()),
                    comment: Some("Units shipped, it's counted".into()),
                    ..definition("shipped", "smallint", false)
                },
            }]),
        ),
        (
            "add nullable column with an awkward name",
            edit(vec![SchemaChange::AddColumn {
                column: definition("new \"col\"", "integer", true),
            }]),
        ),
        (
            "rename column",
            edit(vec![alter(
                "total",
                ColumnDefinition {
                    name: "amount".into(),
                    ..as_is(&info, "total")
                },
                None,
            )]),
        ),
        (
            "change type with conversion",
            edit(vec![alter(
                "total",
                ColumnDefinition {
                    type_name: "numeric(10,2)".into(),
                    ..as_is(&info, "total")
                },
                Some("total::numeric"),
            )]),
        ),
        (
            "change type and default",
            edit(vec![alter(
                "note",
                ColumnDefinition {
                    type_name: "varchar(200)".into(),
                    default: Some("'unknown'".into()),
                    ..as_is(&info, "note")
                },
                None,
            )]),
        ),
        (
            "set not null and comment",
            edit(vec![alter(
                "customer_id",
                ColumnDefinition {
                    nullable: false,
                    comment: Some("Who ordered".into()),
                    ..as_is(&info, "customer_id")
                },
                None,
            )]),
        ),
        (
            "drop default",
            edit(vec![alter(
                "note",
                ColumnDefinition {
                    default: None,
                    ..as_is(&info, "note")
                },
                None,
            )]),
        ),
        (
            "drop column after its key",
            edit(vec![
                SchemaChange::DropUnique {
                    key: key("orders_note_key", &["note"]),
                },
                SchemaChange::DropColumn {
                    name: "note".into(),
                },
            ]),
        ),
    ];
    let mut cases = column_changes;
    cases.extend(constraint_cases(namespace, &edit));
    cases.extend(table_cases(namespace, &info, &edit));
    cases
}

fn constraint_cases(
    namespace: &NamespaceRef,
    edit: &dyn Fn(Vec<SchemaChange>) -> SchemaEdit,
) -> Vec<(&'static str, SchemaEdit)> {
    vec![
        (
            "replace primary key",
            edit(vec![
                SchemaChange::DropPrimaryKey,
                SchemaChange::AddPrimaryKey {
                    key: key("orders_pkey", &["id", "customer_id"]),
                },
            ]),
        ),
        (
            "add unique",
            edit(vec![SchemaChange::AddUnique {
                key: key("orders_customer_total_key", &["customer_id", "total"]),
            }]),
        ),
        (
            "add and drop foreign keys",
            edit(vec![
                SchemaChange::DropForeignKey {
                    foreign_key: existing_foreign_key(namespace),
                },
                SchemaChange::AddForeignKey {
                    foreign_key: ForeignKey {
                        name: Some("orders_customer_fk".into()),
                        on_update: Some(ReferentialAction::Cascade),
                        on_delete: Some(ReferentialAction::SetNull),
                        ..existing_foreign_key(namespace)
                    },
                },
                SchemaChange::AddForeignKey {
                    foreign_key: ForeignKey {
                        name: None,
                        columns: names(&["odd \"name\""]),
                        referenced_table: table(namespace, "odd \"table\""),
                        referenced_columns: names(&["id"]),
                        on_update: None,
                        on_delete: Some(ReferentialAction::Restrict),
                    },
                },
            ]),
        ),
        (
            "add and drop checks",
            edit(vec![
                SchemaChange::DropCheck {
                    check: Check {
                        name: Some("orders_total_check".into()),
                        expression: "total >= 0".into(),
                    },
                },
                SchemaChange::AddCheck {
                    check: Check {
                        name: Some("orders_total_positive".into()),
                        expression: "total > 0 AND note <> 'a;b'".into(),
                    },
                },
            ]),
        ),
        (
            "add and drop indexes",
            edit(vec![
                SchemaChange::DropIndex {
                    index: Index {
                        name: "orders_total_idx".into(),
                        columns: names(&["total"]),
                        unique: false,
                        method: None,
                        predicate: None,
                    },
                },
                SchemaChange::AddIndex {
                    index: Index {
                        name: "orders_customer_total_idx".into(),
                        columns: names(&["customer_id", "total"]),
                        unique: true,
                        method: Some("btree".into()),
                        predicate: None,
                    },
                },
                SchemaChange::AddIndex {
                    index: Index {
                        name: "orders_note_lower_idx".into(),
                        columns: names(&["lower(note)"]),
                        unique: false,
                        method: None,
                        predicate: None,
                    },
                },
            ]),
        ),
    ]
}

fn table_cases(
    namespace: &NamespaceRef,
    info: &TableInfo,
    edit: &dyn Fn(Vec<SchemaChange>) -> SchemaEdit,
) -> Vec<(&'static str, SchemaEdit)> {
    vec![
        (
            "rename table and set comment",
            edit(vec![
                SchemaChange::RenameTable {
                    name: "purchases".into(),
                },
                SchemaChange::SetComment {
                    comment: Some("Every purchase".into()),
                },
            ]),
        ),
        (
            "clear comment",
            edit(vec![SchemaChange::SetComment { comment: None }]),
        ),
        (
            "several changes at once",
            edit(vec![
                SchemaChange::DropIndex {
                    index: info.indexes[0].clone(),
                },
                SchemaChange::AddColumn {
                    column: definition("shipped_at", "timestamp", true),
                },
                alter(
                    "total",
                    ColumnDefinition {
                        name: "amount".into(),
                        nullable: false,
                        ..as_is(info, "total")
                    },
                    None,
                ),
                SchemaChange::AddForeignKey {
                    foreign_key: ForeignKey {
                        name: Some("orders_amount_fk".into()),
                        columns: names(&["amount"]),
                        referenced_table: table(namespace, "amounts"),
                        referenced_columns: names(&["value"]),
                        on_update: None,
                        on_delete: None,
                    },
                },
                SchemaChange::AddIndex {
                    index: Index {
                        name: "orders_amount_idx".into(),
                        columns: names(&["amount"]),
                        unique: false,
                        method: None,
                        predicate: None,
                    },
                },
            ]),
        ),
    ]
}
