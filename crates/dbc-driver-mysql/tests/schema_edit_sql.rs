//! Schema edit plans in this dialect (task 9.5). No database needed: the planner reads the
//! table's `SHOW CREATE TABLE` text, which these tests write by hand.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dbc_core::conformance::{schema_edit_cases, schema_edit_table};
use dbc_core::schema::{Index, NamespaceRef};
use dbc_core::schema_edit::{ColumnDefinition, SchemaChange, SchemaEdit, SchemaPlan};
use dbc_driver_mysql::plan_schema_edit;

/// [`schema_edit_table`] as MySQL 8 shows it, with attributes the model does not hold.
const ORDERS: &str = "CREATE TABLE `orders` (
  `id` int NOT NULL AUTO_INCREMENT,
  `customer_id` int DEFAULT NULL,
  `total` int DEFAULT NULL,
  `note` varchar(100) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin DEFAULT 'none',
  `odd \"name\"` int DEFAULT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `orders_note_key` (`note`),
  KEY `orders_total_idx` (`total`),
  KEY `orders_customer_fk` (`customer_id`),
  CONSTRAINT `orders_customer_fk` FOREIGN KEY (`customer_id`) REFERENCES `customers` (`id`),
  CONSTRAINT `orders_total_check` CHECK ((`total` >= 0))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4";

fn shop() -> NamespaceRef {
    NamespaceRef {
        catalog: None,
        schema: Some("shop".into()),
    }
}

fn render(label: &str, plan: &SchemaPlan) -> String {
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
}

#[test]
fn schema_edit_plans() {
    let rendered: Vec<String> = schema_edit_cases(&shop())
        .into_iter()
        .map(|(label, edit)| {
            let plan = plan_schema_edit(&edit, ORDERS).unwrap_or_else(|e| panic!("{label}: {e}"));
            assert!(!plan.transactional, "{label}");
            render(label, &plan)
        })
        .collect();
    insta::assert_snapshot!(rendered.join("\n\n"));
}

fn alter(name: &str, column: ColumnDefinition) -> SchemaEdit {
    SchemaEdit {
        original: schema_edit_table(&shop()),
        changes: vec![SchemaChange::AlterColumn {
            name: name.into(),
            column,
            using: None,
        }],
    }
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

#[test]
fn renaming_keeps_auto_increment() {
    let plan = plan_schema_edit(
        &alter("id", definition("order_id", "integer", false, None)),
        ORDERS,
    )
    .unwrap();
    assert_eq!(
        plan.statements,
        ["ALTER TABLE `shop`.`orders` CHANGE COLUMN `id` `order_id` int NOT NULL AUTO_INCREMENT"]
    );
}

#[test]
fn an_unchanged_default_is_written_as_the_server_wrote_it() {
    let plan = plan_schema_edit(
        &alter(
            "note",
            definition("note", "varchar(100)", false, Some("'none'")),
        ),
        ORDERS,
    )
    .unwrap();
    assert_eq!(
        plan.statements,
        [
            "ALTER TABLE `shop`.`orders` CHANGE COLUMN `note` `note` varchar(100) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL DEFAULT 'none'"
        ]
    );
}

#[test]
fn a_new_type_drops_the_character_set_and_collation() {
    let plan = plan_schema_edit(
        &alter("note", definition("note", "text", true, Some("'none'"))),
        ORDERS,
    )
    .unwrap();
    assert_eq!(
        plan.statements,
        ["ALTER TABLE `shop`.`orders` CHANGE COLUMN `note` `note` text DEFAULT 'none'"]
    );
    assert!(
        plan.warnings[0].contains("type of note"),
        "{:?}",
        plan.warnings
    );
}

#[test]
fn on_update_survives_a_change() {
    let create = "CREATE TABLE `orders` (
  `id` int NOT NULL,
  `customer_id` int DEFAULT NULL,
  `total` int DEFAULT NULL,
  `note` timestamp NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT 'Seen',
  `odd \"name\"` int DEFAULT NULL
)";
    let mut edit = alter("note", definition("note", "varchar(100)", true, None));
    if let SchemaChange::AlterColumn { column, .. } = &mut edit.changes[0] {
        column.default = Some("'none'".into());
        column.comment = Some("Seen, it's \"quoted\"".into());
    }
    let plan = plan_schema_edit(&edit, create).unwrap();
    assert_eq!(
        plan.statements,
        [
            "ALTER TABLE `shop`.`orders` CHANGE COLUMN `note` `note` timestamp NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT 'Seen, it''s \"quoted\"'"
        ]
    );
}

#[test]
fn comments_with_backslashes_are_refused() {
    let mut edit = alter("total", definition("total", "integer", true, None));
    if let SchemaChange::AlterColumn { column, .. } = &mut edit.changes[0] {
        column.comment = Some("a\\b".into());
    }
    let error = plan_schema_edit(&edit, ORDERS).unwrap_err();
    assert!(error.message.contains("backslash"), "{}", error.message);
}

#[test]
fn partial_indexes_are_refused() {
    let edit = SchemaEdit {
        original: schema_edit_table(&shop()),
        changes: vec![SchemaChange::AddIndex {
            index: Index {
                name: "i".into(),
                columns: vec!["total".into()],
                unique: false,
                method: None,
                predicate: Some("total > 0".into()),
            },
        }],
    };
    let error = plan_schema_edit(&edit, ORDERS).unwrap_err();
    assert!(error.message.contains("partial"), "{}", error.message);
}

#[test]
fn a_column_missing_from_the_create_text_is_an_error() {
    let create = "CREATE TABLE `orders` (`id` int NOT NULL)";
    let error = plan_schema_edit(
        &alter("total", definition("total", "bigint", true, None)),
        create,
    )
    .unwrap_err();
    assert!(error.message.contains("total"), "{}", error.message);
}
