#![allow(clippy::unwrap_used)]

use super::*;
use crate::dialect::quote_with;
use crate::result::TypeCategory;
use crate::schema::{Column, Key, NamespaceRef, ObjectKind};
use crate::sql::SplitterRules;

struct Pg;

impl Dialect for Pg {
    fn quote_identifier(&self, name: &str) -> String {
        quote_with(name, '"')
    }
    fn splitter_rules(&self) -> SplitterRules {
        SplitterRules::default()
    }
    fn placeholder(&self, index: usize, type_name: &str) -> String {
        format!("${index}::text::{type_name}")
    }
}

fn column(name: &str, type_name: &str) -> Column {
    Column {
        name: name.into(),
        type_name: type_name.into(),
        category: TypeCategory::Integer,
        nullable: false,
        default: None,
        comment: None,
        ordinal: 1,
        generated: false,
    }
}

fn info(row_key: Option<&[&str]>) -> TableInfo {
    TableInfo {
        table: TableRef {
            namespace: NamespaceRef {
                catalog: None,
                schema: Some("public".into()),
            },
            name: "order \"items\"".into(),
        },
        kind: ObjectKind::Table,
        comment: None,
        columns: vec![
            column("order_id", "integer"),
            column("line", "integer"),
            column("sku", "text"),
        ],
        primary_key: row_key.map(|k| Key {
            name: None,
            columns: k.iter().map(|c| (*c).to_owned()).collect(),
        }),
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
        unique_constraints: Vec::new(),
        check_constraints: Vec::new(),
        row_key: row_key.map(|k| k.iter().map(|c| (*c).to_owned()).collect()),
        row_id: None,
    }
}

fn request() -> TablePageRequest {
    TablePageRequest {
        table: info(None).table,
        filter: None,
        key: Vec::new(),
        sort: Vec::new(),
        limit: DEFAULT_PAGE_SIZE,
        offset: 0,
    }
}

const TABLE: &str = r#""public"."order ""items""""#;

#[test]
fn a_plain_page_orders_by_the_row_key() {
    let bound = page_sql(&Pg, &info(Some(&["order_id", "line"])), &request()).unwrap();

    assert_eq!(
        bound.sql,
        format!(r#"SELECT * FROM {TABLE} ORDER BY "order_id", "line" LIMIT 200 OFFSET 0"#)
    );
    assert!(bound.params.is_empty());
}

#[test]
fn a_table_without_a_row_key_has_no_order() {
    let bound = page_sql(
        &Pg,
        &info(None),
        &TablePageRequest {
            offset: 400,
            ..request()
        },
    )
    .unwrap();

    assert_eq!(
        bound.sql,
        format!("SELECT * FROM {TABLE} LIMIT 200 OFFSET 400")
    );
}

#[test]
fn filter_key_and_sort_combine_with_bound_values() {
    let page = TablePageRequest {
        filter: Some("  sku LIKE 'A%' OR line > 1 ".into()),
        key: vec![
            KeyValue {
                column: "order_id".into(),
                value: Value::Int(10),
            },
            KeyValue {
                column: "sku".into(),
                value: Value::Null,
            },
        ],
        sort: vec![
            SortKey {
                column: "sku".into(),
                descending: true,
            },
            SortKey {
                column: "line".into(),
                descending: false,
            },
        ],
        limit: 50_000,
        ..request()
    };

    let bound = page_sql(&Pg, &info(Some(&["order_id", "line"])), &page).unwrap();

    assert_eq!(
        bound.sql,
        format!(
            r#"SELECT * FROM {TABLE} WHERE (sku LIKE 'A%' OR line > 1) AND "order_id" = $1::text::integer AND "sku" IS NULL ORDER BY "sku" DESC, "line" LIMIT 10000 OFFSET 0"#
        )
    );
    assert_eq!(bound.params, vec![Value::Int(10)]);
}

#[test]
fn a_filter_with_more_than_one_statement_is_refused() {
    for filter in ["1 = 1; DROP TABLE t", "1 = 1;"] {
        let page = TablePageRequest {
            filter: Some(filter.into()),
            ..request()
        };

        let error = page_sql(&Pg, &info(None), &page).unwrap_err();

        assert_eq!(error.code, DbErrorCode::Syntax, "{filter}");
    }
    let quoted = TablePageRequest {
        filter: Some("sku = ';'".into()),
        ..request()
    };
    assert!(page_sql(&Pg, &info(None), &quoted).is_ok());
}

#[test]
fn unknown_columns_are_refused_rather_than_quoted() {
    let sort = TablePageRequest {
        sort: vec![SortKey {
            column: "nope\" --".into(),
            descending: false,
        }],
        ..request()
    };
    let key = TablePageRequest {
        key: vec![KeyValue {
            column: "nope".into(),
            value: Value::Int(1),
        }],
        ..request()
    };

    assert_eq!(
        page_sql(&Pg, &info(None), &sort).unwrap_err().code,
        DbErrorCode::NotFound
    );
    assert_eq!(
        page_sql(&Pg, &info(None), &key).unwrap_err().code,
        DbErrorCode::NotFound
    );
}

#[test]
fn count_and_cell_statements() {
    let key = [
        KeyValue {
            column: "line".into(),
            value: Value::Int(2),
        },
        KeyValue {
            column: "order_id".into(),
            value: Value::Int(10),
        },
    ];
    let keyed = info(Some(&["order_id", "line"]));

    let count = count_sql(&Pg, &keyed, Some("line > 1"), &[]).unwrap();
    let cell = cell_sql(&Pg, &keyed, &key, "sku").unwrap();

    assert_eq!(
        count.sql,
        format!("SELECT count(*) FROM {TABLE} WHERE (line > 1)")
    );
    assert_eq!(
        cell.sql,
        format!(
            r#"SELECT "sku" FROM {TABLE} WHERE "line" = $1::text::integer AND "order_id" = $2::text::integer"#
        )
    );
    assert_eq!(cell.params, vec![Value::Int(2), Value::Int(10)]);
}

#[test]
fn a_cell_needs_the_whole_row_key() {
    let partial = [KeyValue {
        column: "line".into(),
        value: Value::Int(2),
    }];

    let without_key = cell_sql(&Pg, &info(None), &partial, "sku").unwrap_err();
    let partial_key =
        cell_sql(&Pg, &info(Some(&["order_id", "line"])), &partial, "sku").unwrap_err();
    let unknown = cell_sql(&Pg, &info(Some(&["line"])), &partial, "nope").unwrap_err();

    assert_eq!(without_key.code, DbErrorCode::Unsupported);
    assert_eq!(partial_key.code, DbErrorCode::Unsupported);
    assert_eq!(unknown.code, DbErrorCode::NotFound);
}

#[test]
fn row_counts_serialise_with_their_kind() {
    assert_eq!(
        serde_json::to_value(RowCount::Estimate(5)).unwrap(),
        serde_json::json!({"kind": "estimate", "rows": "5"})
    );
    assert_eq!(
        serde_json::to_value(RowCount::Unknown).unwrap(),
        serde_json::json!({"kind": "unknown"})
    );
    let exact: RowCount =
        serde_json::from_value(serde_json::json!({"kind": "exact", "rows": "7"})).unwrap();
    assert_eq!(exact, RowCount::Exact(7));
}

#[test]
fn a_whole_table_selects_named_columns_without_a_limit() {
    let keyed = info(Some(&["order_id", "line"]));

    let all = select_sql(&Pg, &keyed, &request(), None).unwrap();
    let named = select_sql(
        &Pg,
        &keyed,
        &request(),
        Some(&["sku".to_owned(), "line".to_owned()]),
    )
    .unwrap();
    let unknown = select_sql(&Pg, &keyed, &request(), Some(&["nope".to_owned()])).unwrap_err();

    assert_eq!(
        all.sql,
        format!(r#"SELECT * FROM {TABLE} ORDER BY "order_id", "line""#)
    );
    assert_eq!(
        named.sql,
        format!(r#"SELECT "sku", "line" FROM {TABLE} ORDER BY "order_id", "line""#)
    );
    assert_eq!(unknown.code, DbErrorCode::NotFound);
}

#[test]
fn an_implicit_row_id_is_selected_first_ordered_by_and_addresses_cells() {
    let info = TableInfo {
        row_id: Some("rowid".into()),
        row_key: Some(vec!["rowid".into()]),
        ..info(None)
    };

    let page = page_sql(&Pg, &info, &request()).unwrap();
    let cell = cell_sql(
        &Pg,
        &info,
        &[KeyValue {
            column: "rowid".into(),
            value: Value::Int(7),
        }],
        "sku",
    )
    .unwrap();

    assert_eq!(
        page.sql,
        format!(r#"SELECT "rowid", * FROM {TABLE} ORDER BY "rowid" LIMIT 200 OFFSET 0"#)
    );
    assert!(
        cell.sql.contains(r#""rowid" = $1::text::INTEGER"#),
        "{}",
        cell.sql
    );
}

#[test]
fn a_real_key_wins_over_the_row_id() {
    let info = TableInfo {
        row_id: Some("rowid".into()),
        ..info(Some(&["order_id"]))
    };

    assert_eq!(info.choose_row_key(), Some(vec!["order_id".to_owned()]));
    assert!(!info.uses_row_id());
    assert!(
        page_sql(&Pg, &info, &request())
            .unwrap()
            .sql
            .starts_with("SELECT * ")
    );
}
