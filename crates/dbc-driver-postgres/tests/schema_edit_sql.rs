//! Schema edit plans in this dialect (task 9.4). No database needed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dbc_core::conformance::{schema_edit_cases, schema_edit_table};
use dbc_core::schema::NamespaceRef;
use dbc_core::schema_edit::{ColumnDefinition, SchemaChange, SchemaEdit};
use dbc_driver_postgres::plan_schema_edit;

fn public() -> NamespaceRef {
    NamespaceRef {
        catalog: None,
        schema: Some("public".into()),
    }
}

#[test]
fn schema_edit_plans() {
    let rendered: Vec<String> = schema_edit_cases(&public())
        .into_iter()
        .map(|(label, edit)| {
            let plan = plan_schema_edit(&edit).unwrap();
            assert!(plan.transactional, "{label}");
            assert!(plan.before.is_empty() && plan.after.is_empty(), "{label}");
            let destructive = if plan.destructive {
                " (destructive)"
            } else {
                ""
            };
            let warnings: String = plan
                .warnings
                .iter()
                .map(|w| format!("-- warning: {w}\n"))
                .collect();
            format!(
                "-- {label}{destructive}\n{warnings}{};",
                plan.statements.join(";\n")
            )
        })
        .collect();
    insta::assert_snapshot!(rendered.join("\n\n"));
}

#[test]
fn identity_columns_keep_their_default() {
    let mut original = schema_edit_table(&public());
    original.columns[0].default = Some("GENERATED ALWAYS AS IDENTITY".into());
    let edit = SchemaEdit {
        original,
        changes: vec![SchemaChange::AlterColumn {
            name: "id".into(),
            column: ColumnDefinition {
                name: "id".into(),
                type_name: "integer".into(),
                nullable: false,
                default: Some("1".into()),
                comment: None,
            },
            using: None,
        }],
    };
    let error = plan_schema_edit(&edit).unwrap_err();
    assert!(error.message.contains("identity"), "{}", error.message);
}

#[test]
fn an_unnamed_constraint_cannot_be_dropped() {
    let mut original = schema_edit_table(&public());
    original.check_constraints[0].name = None;
    let check = original.check_constraints[0].clone();
    let edit = SchemaEdit {
        original,
        changes: vec![SchemaChange::DropCheck { check }],
    };
    let error = plan_schema_edit(&edit).unwrap_err();
    assert!(error.message.contains("no name"), "{}", error.message);
}

#[test]
fn an_alter_that_changes_nothing_is_refused() {
    let original = schema_edit_table(&public());
    let edit = SchemaEdit {
        original,
        changes: vec![SchemaChange::AlterColumn {
            name: "total".into(),
            column: ColumnDefinition {
                name: "total".into(),
                type_name: "integer".into(),
                nullable: true,
                default: None,
                comment: None,
            },
            using: None,
        }],
    };
    let error = plan_schema_edit(&edit).unwrap_err();
    assert!(
        error.message.contains("Nothing to apply"),
        "{}",
        error.message
    );
}
