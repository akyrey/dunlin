#![allow(clippy::unwrap_used)]

use super::*;
use crate::dialect::quote_with;
use crate::result::TypeCategory;
use crate::schema::{Check, Column, ForeignKey, Index, Key, NamespaceRef, ObjectKind, TableRef};
use crate::sql::SplitterRules;

struct Ansi;

impl Dialect for Ansi {
    fn quote_identifier(&self, name: &str) -> String {
        quote_with(name, '"')
    }
    fn splitter_rules(&self) -> SplitterRules {
        SplitterRules::default()
    }
    fn placeholder(&self, index: usize, _: &str) -> String {
        format!("${index}")
    }
}

fn table_ref(name: &str) -> TableRef {
    TableRef {
        namespace: NamespaceRef {
            catalog: None,
            schema: Some("public".into()),
        },
        name: name.into(),
    }
}

fn column(name: &str, ordinal: u32, nullable: bool) -> Column {
    Column {
        name: name.into(),
        type_name: "integer".into(),
        category: TypeCategory::Integer,
        nullable,
        default: None,
        comment: None,
        ordinal,
        generated: false,
    }
}

fn key(name: Option<&str>, columns: &[&str]) -> Key {
    Key {
        name: name.map(str::to_owned),
        columns: columns.iter().map(|c| (*c).to_owned()).collect(),
    }
}

fn index(name: &str, columns: &[&str]) -> Index {
    Index {
        name: name.into(),
        columns: columns.iter().map(|c| (*c).to_owned()).collect(),
        unique: false,
        method: None,
        predicate: None,
    }
}

fn foreign_key(columns: &[&str], table: &str, referenced: &[&str]) -> ForeignKey {
    ForeignKey {
        name: Some("orders_customer_fk".into()),
        columns: columns.iter().map(|c| (*c).to_owned()).collect(),
        referenced_table: table_ref(table),
        referenced_columns: referenced.iter().map(|c| (*c).to_owned()).collect(),
        on_update: None,
        on_delete: None,
    }
}

/// `orders(id pk, customer_id fk -> customers.id, total, note)`, an index on `total`, a unique
/// key on `note` and a check on `total`.
fn orders() -> TableInfo {
    TableInfo {
        table: table_ref("orders"),
        kind: ObjectKind::Table,
        comment: None,
        columns: vec![
            column("id", 1, false),
            column("customer_id", 2, true),
            column("total", 3, true),
            column("note", 4, true),
        ],
        primary_key: Some(key(Some("orders_pkey"), &["id"])),
        indexes: vec![index("orders_total_idx", &["total"])],
        foreign_keys: vec![foreign_key(&["customer_id"], "customers", &["id"])],
        unique_constraints: vec![key(Some("orders_note_key"), &["note"])],
        check_constraints: vec![Check {
            name: Some("orders_total_check".into()),
            expression: "total >= 0".into(),
        }],
        row_key: Some(vec!["id".into()]),
        row_id: None,
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

fn edit(changes: Vec<SchemaChange>) -> SchemaEdit {
    SchemaEdit {
        original: orders(),
        changes,
    }
}

fn expected(changes: Vec<SchemaChange>) -> Result<TableInfo, DbError> {
    expected_table(&Ansi, &edit(changes))
}

fn refusal(changes: Vec<SchemaChange>) -> String {
    let error = expected(changes).unwrap_err();
    assert_eq!(error.code, DbErrorCode::Execution, "{error:?}");
    error.message
}

fn column_names(info: &TableInfo) -> Vec<&str> {
    info.columns.iter().map(|c| c.name.as_str()).collect()
}

#[test]
fn an_edit_without_changes_is_refused() {
    assert!(refusal(vec![]).contains("Nothing to apply"));
}

#[test]
fn views_cannot_be_edited() {
    let mut original = orders();
    original.kind = ObjectKind::View;
    let edit = SchemaEdit {
        original,
        changes: vec![SchemaChange::RenameTable { name: "o".into() }],
    };
    let error = expected_table(&Ansi, &edit).unwrap_err();
    assert!(error.message.contains("Only tables"));
}

#[test]
fn an_added_column_is_appended_with_the_next_ordinal() {
    let info = expected(vec![SchemaChange::AddColumn {
        column: ColumnDefinition {
            default: Some("0".into()),
            comment: Some("Units shipped".into()),
            ..definition("shipped", "smallint", false)
        },
    }])
    .unwrap();
    let added = info.columns.last().unwrap();
    assert_eq!(column_names(&info).len(), 5);
    assert_eq!(added.name, "shipped");
    assert_eq!(added.type_name, "smallint");
    assert_eq!(added.ordinal, 5);
    assert!(!added.nullable);
    assert_eq!(added.default.as_deref(), Some("0"));
    assert_eq!(added.comment.as_deref(), Some("Units shipped"));
    assert!(!added.generated);
}

#[test]
fn a_column_name_may_not_be_taken_twice() {
    assert!(
        refusal(vec![SchemaChange::AddColumn {
            column: definition("total", "integer", true),
        }])
        .contains("already has a column total")
    );
    assert!(
        refusal(vec![SchemaChange::AlterColumn {
            name: "note".into(),
            column: definition("total", "text", true),
            using: None,
        }])
        .contains("already has a column total")
    );
}

#[test]
fn empty_names_and_types_are_refused() {
    assert!(
        refusal(vec![SchemaChange::AddColumn {
            column: definition("  ", "integer", true),
        }])
        .contains("name")
    );
    assert!(
        refusal(vec![SchemaChange::AddColumn {
            column: definition("x", "", true),
        }])
        .contains("type")
    );
    assert!(refusal(vec![SchemaChange::RenameTable { name: "".into() }]).contains("name"));
}

#[test]
fn renaming_a_column_follows_it_into_keys_indexes_and_foreign_keys() {
    let mut original = orders();
    original.foreign_keys.push(ForeignKey {
        name: Some("orders_parent_fk".into()),
        ..foreign_key(&["total"], "orders", &["id"])
    });
    let edit = SchemaEdit {
        original,
        changes: vec![
            SchemaChange::AlterColumn {
                name: "id".into(),
                column: definition("order_id", "bigint", false),
                using: None,
            },
            SchemaChange::AlterColumn {
                name: "total".into(),
                column: definition("amount", "numeric(10,2)", true),
                using: Some("total::numeric".into()),
            },
        ],
    };
    let info = expected_table(&Ansi, &edit).unwrap();
    assert_eq!(
        column_names(&info),
        ["order_id", "customer_id", "amount", "note"]
    );
    assert_eq!(info.columns[0].type_name, "bigint");
    assert_eq!(info.columns[0].ordinal, 1);
    assert_eq!(info.primary_key.unwrap().columns, ["order_id"]);
    assert_eq!(info.indexes[0].columns, ["amount"]);
    let parent = &info.foreign_keys[1];
    assert_eq!(parent.columns, ["amount"]);
    assert_eq!(parent.referenced_columns, ["order_id"], "self-reference");
    assert_eq!(
        info.foreign_keys[0].referenced_columns,
        ["id"],
        "other table"
    );
    assert_eq!(info.row_key, Some(vec!["order_id".into()]));
}

#[test]
fn a_changed_type_resets_the_category() {
    let info = expected(vec![SchemaChange::AlterColumn {
        name: "note".into(),
        column: definition("note", "text", true),
        using: None,
    }])
    .unwrap();
    assert_eq!(info.columns[3].category, TypeCategory::Other);
    let same = expected(vec![SchemaChange::AlterColumn {
        name: "total".into(),
        column: definition("total", "integer", false),
        using: None,
    }])
    .unwrap();
    assert_eq!(same.columns[2].category, TypeCategory::Integer);
    assert!(!same.columns[2].nullable);
}

#[test]
fn generated_columns_cannot_be_altered() {
    let mut original = orders();
    original.columns[3].generated = true;
    let edit = SchemaEdit {
        original,
        changes: vec![SchemaChange::AlterColumn {
            name: "note".into(),
            column: definition("note2", "integer", true),
            using: None,
        }],
    };
    let error = expected_table(&Ansi, &edit).unwrap_err();
    assert!(error.message.contains("generated"), "{}", error.message);
}

#[test]
fn unknown_columns_are_refused() {
    assert!(
        refusal(vec![SchemaChange::DropColumn {
            name: "missing".into()
        }])
        .contains("no column missing")
    );
    assert!(
        refusal(vec![SchemaChange::AlterColumn {
            name: "missing".into(),
            column: definition("x", "integer", true),
            using: None,
        }])
        .contains("no column missing")
    );
}

#[test]
fn a_column_in_use_cannot_be_dropped() {
    for (name, used_by) in [
        ("id", "primary key"),
        ("customer_id", "orders_customer_fk"),
        ("total", "orders_total_idx"),
        ("note", "orders_note_key"),
    ] {
        let message = refusal(vec![SchemaChange::DropColumn { name: name.into() }]);
        assert!(message.contains(used_by), "{name}: {message}");
    }
}

#[test]
fn a_column_can_be_dropped_after_what_uses_it() {
    let original = orders();
    let info = expected(vec![
        SchemaChange::DropIndex {
            index: original.indexes[0].clone(),
        },
        SchemaChange::DropColumn {
            name: "total".into(),
        },
    ])
    .unwrap();
    assert_eq!(column_names(&info), ["id", "customer_id", "note"]);
    assert!(info.indexes.is_empty());
}

#[test]
fn dropping_the_last_column_is_refused() {
    let mut original = orders();
    original.columns.truncate(1);
    original.primary_key = None;
    original.indexes.clear();
    original.foreign_keys.clear();
    original.unique_constraints.clear();
    let edit = SchemaEdit {
        original,
        changes: vec![SchemaChange::DropColumn { name: "id".into() }],
    };
    let error = expected_table(&Ansi, &edit).unwrap_err();
    assert!(error.message.contains("at least one column"));
}

#[test]
fn a_table_has_at_most_one_primary_key() {
    assert!(
        refusal(vec![SchemaChange::AddPrimaryKey {
            key: key(None, &["note"]),
        }])
        .contains("already has a primary key")
    );
    let info = expected(vec![
        SchemaChange::DropPrimaryKey,
        SchemaChange::AddPrimaryKey {
            key: key(None, &["id", "customer_id"]),
        },
    ])
    .unwrap();
    assert_eq!(info.primary_key.unwrap().columns, ["id", "customer_id"]);
    let mut original = orders();
    original.primary_key = None;
    let edit = SchemaEdit {
        original,
        changes: vec![SchemaChange::DropPrimaryKey],
    };
    assert!(
        expected_table(&Ansi, &edit)
            .unwrap_err()
            .message
            .contains("has no primary key")
    );
}

#[test]
fn keys_and_indexes_need_known_distinct_columns() {
    assert!(
        refusal(vec![SchemaChange::AddUnique {
            key: key(None, &[]),
        }])
        .contains("at least one column")
    );
    assert!(
        refusal(vec![SchemaChange::AddUnique {
            key: key(None, &["nope"]),
        }])
        .contains("no column nope")
    );
    assert!(
        refusal(vec![SchemaChange::AddUnique {
            key: key(None, &["id", "id"]),
        }])
        .contains("twice")
    );
    assert!(
        refusal(vec![SchemaChange::AddIndex {
            index: index("", &["total"]),
        }])
        .contains("name")
    );
}

#[test]
fn index_columns_may_be_expressions() {
    let info = expected(vec![SchemaChange::AddIndex {
        index: Index {
            predicate: Some("note IS NOT NULL".into()),
            ..index("orders_lower_note", &["lower(note)"])
        },
    }])
    .unwrap();
    assert_eq!(info.indexes[1].columns, ["lower(note)"]);
}

#[test]
fn index_methods_are_plain_names() {
    let with_method = |method: &str| SchemaChange::AddIndex {
        index: Index {
            method: Some(method.into()),
            ..index("i", &["note"])
        },
    };
    expected(vec![with_method("gin_trgm")]).unwrap();
    assert!(refusal(vec![with_method("btree (x); --")]).contains("plain name"));
    assert!(refusal(vec![with_method("")]).contains("plain name"));
}

#[test]
fn index_names_are_unique() {
    assert!(
        refusal(vec![SchemaChange::AddIndex {
            index: index("orders_total_idx", &["note"]),
        }])
        .contains("already has an index orders_total_idx")
    );
}

#[test]
fn foreign_keys_pair_columns_with_referenced_columns() {
    assert!(
        refusal(vec![SchemaChange::AddForeignKey {
            foreign_key: foreign_key(&["customer_id", "note"], "customers", &["id"]),
        }])
        .contains("same number")
    );
    let info = expected(vec![SchemaChange::AddForeignKey {
        foreign_key: ForeignKey {
            name: None,
            on_delete: Some(crate::schema::ReferentialAction::Cascade),
            ..foreign_key(&["note"], "notes", &["id"])
        },
    }])
    .unwrap();
    assert_eq!(info.foreign_keys.len(), 2);
}

#[test]
fn dropped_objects_must_exist_in_the_original() {
    let mut other = index("orders_total_idx", &["note"]);
    assert!(
        refusal(vec![SchemaChange::DropIndex {
            index: other.clone()
        }])
        .contains("no such index")
    );
    other.name = "missing".into();
    assert!(refusal(vec![SchemaChange::DropIndex { index: other }]).contains("no such index"));
    assert!(
        refusal(vec![SchemaChange::DropUnique {
            key: key(None, &["note"]),
        }])
        .contains("no such unique")
    );
    assert!(
        refusal(vec![SchemaChange::DropForeignKey {
            foreign_key: foreign_key(&["total"], "customers", &["id"]),
        }])
        .contains("no such foreign key")
    );
    assert!(
        refusal(vec![SchemaChange::DropCheck {
            check: Check {
                name: None,
                expression: "total > 1".into(),
            },
        }])
        .contains("no such check")
    );
}

#[test]
fn constraints_are_dropped_by_value() {
    let original = orders();
    let info = expected(vec![
        SchemaChange::DropUnique {
            key: original.unique_constraints[0].clone(),
        },
        SchemaChange::DropForeignKey {
            foreign_key: original.foreign_keys[0].clone(),
        },
        SchemaChange::DropCheck {
            check: original.check_constraints[0].clone(),
        },
        SchemaChange::DropColumn {
            name: "note".into(),
        },
        SchemaChange::DropColumn {
            name: "customer_id".into(),
        },
    ])
    .unwrap();
    assert!(info.unique_constraints.is_empty());
    assert!(info.foreign_keys.is_empty());
    assert!(info.check_constraints.is_empty());
    assert_eq!(column_names(&info), ["id", "total"]);
}

#[test]
fn renaming_the_table_and_setting_its_comment() {
    let info = expected(vec![
        SchemaChange::RenameTable {
            name: "purchases".into(),
        },
        SchemaChange::SetComment {
            comment: Some("All purchases".into()),
        },
    ])
    .unwrap();
    assert_eq!(info.table.name, "purchases");
    assert_eq!(info.table.namespace, orders().table.namespace);
    assert_eq!(info.comment.as_deref(), Some("All purchases"));
}

#[test]
fn user_sql_must_be_one_expression() {
    let cases = [
        SchemaChange::AddColumn {
            column: definition("x", "integer; DROP TABLE orders", true),
        },
        SchemaChange::AddColumn {
            column: ColumnDefinition {
                default: Some("1; DROP TABLE orders".into()),
                ..definition("x", "integer", true)
            },
        },
        SchemaChange::AlterColumn {
            name: "total".into(),
            column: definition("total", "bigint", true),
            using: Some("total;".into()),
        },
        SchemaChange::AddCheck {
            check: Check {
                name: None,
                expression: "total > 0); DROP TABLE orders; --".into(),
            },
        },
        SchemaChange::AddIndex {
            index: Index {
                predicate: Some("note IS NULL; DELETE FROM orders".into()),
                ..index("i", &["note"])
            },
        },
        SchemaChange::AddIndex {
            index: index("i", &["lower(note)); DROP TABLE orders; --"]),
        },
    ];
    for change in cases {
        let message = refusal(vec![change.clone()]);
        assert!(message.contains("one expression"), "{change:?}: {message}");
    }
}

#[test]
fn semicolons_inside_strings_are_fine() {
    let info = expected(vec![SchemaChange::AddColumn {
        column: ColumnDefinition {
            default: Some("'a;b'".into()),
            ..definition("x", "text", true)
        },
    }])
    .unwrap();
    assert_eq!(info.columns[4].default.as_deref(), Some("'a;b'"));
}

#[test]
fn unbalanced_parentheses_are_refused() {
    let message = refusal(vec![SchemaChange::AddCheck {
        check: Check {
            name: None,
            expression: "total > 0)".into(),
        },
    }]);
    assert!(message.contains("one expression"), "{message}");
}

#[test]
fn destructive_changes_are_recognised() {
    let original = orders();
    assert!(SchemaChange::DropColumn { name: "x".into() }.is_destructive());
    assert!(SchemaChange::DropPrimaryKey.is_destructive());
    assert!(
        SchemaChange::DropIndex {
            index: original.indexes[0].clone()
        }
        .is_destructive()
    );
    assert!(
        !SchemaChange::AddColumn {
            column: definition("x", "integer", true)
        }
        .is_destructive()
    );
    assert!(
        !SchemaChange::SetComment { comment: None }.is_destructive(),
        "a comment is metadata"
    );
}

#[test]
fn the_wire_format_is_tagged_by_kind() {
    let change = SchemaChange::DropColumn { name: "x".into() };
    let json = serde_json::to_value(&change).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "kind": "drop_column", "name": "x" })
    );
    let back: SchemaChange = serde_json::from_value(json).unwrap();
    assert_eq!(back, change);
}

#[test]
fn table_states_hold_the_table_before_each_change_and_after_the_last() {
    let states = table_states(
        &Ansi,
        &edit(vec![
            SchemaChange::RenameTable { name: "a".into() },
            SchemaChange::RenameTable { name: "b".into() },
        ]),
    )
    .unwrap();
    let names: Vec<&str> = states.iter().map(|s| s.table.name.as_str()).collect();
    assert_eq!(names, ["orders", "a", "b"]);
}

mod render_tests {
    use super::*;
    use crate::schema::ReferentialAction;
    use crate::schema_edit::render::*;

    #[test]
    fn clauses_quote_every_name() {
        let pk = key(Some("p\"k"), &["a", "b c"]);
        assert_eq!(
            primary_key_clause(&Ansi, &pk),
            r#"CONSTRAINT "p""k" PRIMARY KEY ("a", "b c")"#
        );
        assert_eq!(unique_clause(&Ansi, &key(None, &["a"])), r#"UNIQUE ("a")"#);
        let check = Check {
            name: Some("".into()),
            expression: "a > 0".into(),
        };
        assert_eq!(check_clause(&Ansi, &check), "CHECK (a > 0)");
    }

    #[test]
    fn foreign_keys_name_their_actions() {
        let fk = ForeignKey {
            on_update: Some(ReferentialAction::Cascade),
            on_delete: Some(ReferentialAction::SetNull),
            ..foreign_key(&["customer_id"], "customers", &["id"])
        };
        assert_eq!(
            foreign_key_clause(&Ansi, &fk),
            r#"CONSTRAINT "orders_customer_fk" FOREIGN KEY ("customer_id") REFERENCES "public"."customers" ("id") ON UPDATE CASCADE ON DELETE SET NULL"#
        );
    }

    #[test]
    fn identity_defaults_are_written_as_they_are() {
        assert_eq!(default_clause("0"), "DEFAULT 0");
        assert_eq!(
            default_clause("GENERATED ALWAYS AS IDENTITY"),
            "GENERATED ALWAYS AS IDENTITY"
        );
        assert!(is_identity_or_generated("generated by default as identity"));
        assert!(!is_identity_or_generated("'generated'"));
    }

    #[test]
    fn index_parts_quote_columns_and_wrap_expressions() {
        let parts = vec!["total".to_owned(), "lower(note)".to_owned()];
        assert_eq!(
            index_parts(&Ansi, &orders(), &parts),
            r#""total", (lower(note))"#
        );
    }
}
