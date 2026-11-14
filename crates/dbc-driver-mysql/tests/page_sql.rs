//! `SELECT` statements for table pages in this dialect (task 6.2). No database needed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dbc_core::conformance::page_sql_cases;
use dbc_core::data;
use dbc_core::schema::NamespaceRef;
use dbc_driver_mysql::MysqlDialect;

#[test]
fn page_statements() {
    let namespace = NamespaceRef {
        catalog: None,
        schema: Some("shop".into()),
    };

    for (name, info, request) in page_sql_cases(&namespace) {
        let bound = data::page_sql(&MysqlDialect, &info, &request).unwrap();

        insta::assert_snapshot!(name, format!("{}\n{:?}", bound.sql, bound.params));
    }
}
