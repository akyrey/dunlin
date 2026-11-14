//! Change statements and literals in this dialect (task 7.1). No database needed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dbc_core::changes;
use dbc_core::conformance::{change_cases, literal_cases};
use dbc_core::dialect::Dialect;
use dbc_core::schema::NamespaceRef;
use dbc_driver_mysql::MysqlDialect;

#[test]
fn change_statements() {
    let namespace = NamespaceRef {
        catalog: None,
        schema: Some("shop".into()),
    };
    let set = change_cases(
        &namespace,
        &[
            ("id", "int"),
            ("line", "int"),
            ("select", "int"),
            ("spaced \"name\"", "varchar(20)"),
            ("note", "text"),
        ],
    );

    let rendered: Vec<String> = changes::plan(&MysqlDialect, &set)
        .unwrap()
        .iter()
        .map(|s| {
            let bound = s.bound(&MysqlDialect);
            format!(
                "{}\n  params: {:?}\n  preview: {}",
                bound.sql,
                bound.params,
                s.preview(&MysqlDialect)
            )
        })
        .collect();

    insta::assert_snapshot!(rendered.join("\n\n"));
}

#[test]
fn literals() {
    let rendered: Vec<String> = literal_cases()
        .into_iter()
        .map(|(name, value, type_name)| {
            format!("{name}: {}", MysqlDialect.render_literal(&value, type_name))
        })
        .collect();

    insta::assert_snapshot!(rendered.join("\n"));
}
