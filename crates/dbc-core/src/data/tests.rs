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
        after: Vec::new(),
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
            r#"SELECT * FROM {TABLE} WHERE (sku LIKE 'A%' OR line > 1) AND "order_id" = $1::text::integer AND "sku" IS NULL ORDER BY "sku" DESC, "line", "order_id" LIMIT 10000 OFFSET 0"#
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

fn sort_by(column: &str, descending: bool) -> SortKey {
    SortKey {
        column: column.into(),
        descending,
    }
}

#[test]
fn a_sort_ends_with_the_row_key_so_pages_are_stable() {
    let keyed = info(Some(&["order_id", "line"]));

    assert_eq!(
        page_order(&keyed, &[sort_by("line", true)]),
        vec![sort_by("line", true), sort_by("order_id", false)]
    );
    assert_eq!(
        page_order(&info(None), &[sort_by("sku", false)]),
        vec![sort_by("sku", false)]
    );
}

#[test]
fn only_a_total_order_on_not_null_columns_can_seek() {
    let keyed = info(Some(&["order_id", "line"]));
    let mut nullable = keyed.clone();
    nullable.columns[2].nullable = true;

    assert_eq!(
        keyset_order(&keyed, &[]),
        Some(vec![sort_by("order_id", false), sort_by("line", false)])
    );
    assert_eq!(keyset_order(&info(None), &[]), None);
    assert_eq!(keyset_order(&nullable, &[sort_by("sku", false)]), None);
    assert!(keyset_order(&nullable, &[sort_by("line", true)]).is_some());
}

#[test]
fn a_page_after_a_row_seeks_past_it_without_an_offset() {
    let keyed = info(Some(&["order_id"]));
    let page = TablePageRequest {
        after: vec![Value::Int(42)],
        offset: 400,
        ..request()
    };

    let bound = page_sql(&Pg, &keyed, &page).unwrap();

    assert_eq!(
        bound.sql,
        format!(
            r#"SELECT * FROM {TABLE} WHERE "order_id" > $1::text::integer ORDER BY "order_id" LIMIT 200"#
        )
    );
    assert_eq!(bound.params, vec![Value::Int(42)]);
}

#[test]
fn a_seek_over_several_columns_follows_each_direction() {
    let keyed = info(Some(&["order_id", "line"]));
    let page = TablePageRequest {
        filter: Some("sku <> 'x'".into()),
        sort: vec![sort_by("line", true)],
        after: vec![Value::Int(2), Value::Int(10)],
        ..request()
    };

    let bound = page_sql(&Pg, &keyed, &page).unwrap();

    let int = |n: usize| format!("${n}::text::integer");
    assert_eq!(
        bound.sql,
        format!(
            r#"SELECT * FROM {TABLE} WHERE (sku <> 'x') AND "line" <= {} AND ("line" < {} OR ("line" = {} AND "order_id" > {})) ORDER BY "line" DESC, "order_id" LIMIT 200"#,
            int(1),
            int(2),
            int(3),
            int(4)
        )
    );
    assert_eq!(
        bound.params,
        vec![Value::Int(2), Value::Int(2), Value::Int(2), Value::Int(10)]
    );
}

#[test]
fn a_seek_needs_a_value_per_order_column_and_a_seekable_order() {
    let keyed = info(Some(&["order_id", "line"]));
    let short = TablePageRequest {
        after: vec![Value::Int(1)],
        ..request()
    };
    let unkeyed = TablePageRequest {
        after: vec![Value::Int(1)],
        ..request()
    };
    let null = TablePageRequest {
        after: vec![Value::Int(1), Value::Null],
        ..request()
    };

    for (table, page) in [(&keyed, short), (&info(None), unkeyed), (&keyed, null)] {
        let code = page_sql(&Pg, table, &page).unwrap_err().code;
        assert_eq!(code, DbErrorCode::Unsupported);
    }
}

#[test]
fn the_cursor_is_the_order_values_of_a_row() {
    let keyed = info(Some(&["order_id", "line"]));
    let meta = |name: &str| crate::result::ColumnMeta {
        name: name.into(),
        type_name: "integer".into(),
        category: TypeCategory::Integer,
        nullable: None,
    };
    let page = ResultSet {
        columns: vec![meta("sku"), meta("line"), meta("order_id")],
        rows: vec![vec![Value::Text("a".into()), Value::Int(3), Value::Int(7)]],
        truncated: false,
    };

    assert_eq!(
        cursor(&keyed, &[sort_by("sku", true)], &page, 0),
        Some(vec![Value::Text("a".into()), Value::Int(7), Value::Int(3)])
    );
    assert_eq!(cursor(&keyed, &[], &page, 1), None);
    assert_eq!(cursor(&info(None), &[], &page, 0), None);
}

mod running {
    use super::*;
    use crate::fake::{Scripted, affected, block_on, rows};

    fn keyed() -> TableInfo {
        info(Some(&["order_id", "line"]))
    }

    #[test]
    fn a_page_is_the_rows_of_its_statement() {
        let connection = Scripted::answering(vec![rows(vec![Value::Int(1), Value::Int(2)])]);

        let page = block_on(fetch_page(&connection, &Pg, &keyed(), &request())).unwrap();

        assert_eq!(page.rows.len(), 2);
        assert!(connection.log()[0].starts_with("SELECT * FROM"));
    }

    #[test]
    fn a_statement_without_rows_is_an_internal_error() {
        let connection = Scripted::answering(vec![affected(3)]);

        let error = block_on(fetch_page(&connection, &Pg, &keyed(), &request())).unwrap_err();

        assert_eq!(error.code, DbErrorCode::Internal);
    }

    #[test]
    fn counts_are_estimated_only_without_a_filter() {
        let estimating = Scripted {
            estimate: Some(1200),
            ..Scripted::default()
        };
        let none = Scripted::default();

        let estimate = block_on(count_rows(&estimating, &Pg, &keyed(), None, &[], false));
        let blank = block_on(count_rows(
            &estimating,
            &Pg,
            &keyed(),
            Some("  "),
            &[],
            false,
        ));
        let filtered = block_on(count_rows(
            &estimating,
            &Pg,
            &keyed(),
            Some("sku = 'a'"),
            &[],
            false,
        ));
        let unknown = block_on(count_rows(&none, &Pg, &keyed(), None, &[], false));

        assert_eq!(estimate.unwrap(), RowCount::Estimate(1200));
        assert_eq!(blank.unwrap(), RowCount::Estimate(1200));
        assert_eq!(filtered.unwrap(), RowCount::Unknown);
        assert_eq!(unknown.unwrap(), RowCount::Unknown);
        assert!(estimating.log().is_empty(), "estimates run no statement");
    }

    #[test]
    fn an_exact_count_reads_whatever_number_type_the_server_returns() {
        let cases = [
            (Value::Int(7), Some(7)),
            (Value::UInt(8), Some(8)),
            (Value::Decimal("9".into()), Some(9)),
            (Value::Text("10".into()), Some(10)),
            (Value::Int(-1), None),
            (Value::Null, None),
        ];
        for (value, expected) in cases {
            let connection = Scripted::answering(vec![rows(vec![value.clone()])]);

            let count = block_on(count_rows(&connection, &Pg, &keyed(), None, &[], true));

            match expected {
                Some(n) => assert_eq!(count.unwrap(), RowCount::Exact(n), "{value:?}"),
                None => assert_eq!(count.unwrap_err().code, DbErrorCode::Internal, "{value:?}"),
            }
            assert!(connection.log()[0].starts_with("SELECT count(*)"));
        }
    }

    #[test]
    fn a_cell_is_read_by_its_row_key_or_reported_gone() {
        let key = vec![
            KeyValue {
                column: "order_id".into(),
                value: Value::Int(1),
            },
            KeyValue {
                column: "line".into(),
                value: Value::Int(2),
            },
        ];
        let found = Scripted::answering(vec![rows(vec![Value::Text("SKU-1".into())])]);
        let gone = Scripted::answering(vec![rows(vec![])]);

        let value = block_on(fetch_cell(&found, &Pg, &keyed(), &key, "sku"));
        let missing = block_on(fetch_cell(&gone, &Pg, &keyed(), &key, "sku"));

        assert_eq!(value.unwrap(), Value::Text("SKU-1".into()));
        assert_eq!(missing.unwrap_err().code, DbErrorCode::NotFound);
        assert_eq!(
            found.params.lock().unwrap()[0],
            vec![Value::Int(1), Value::Int(2)]
        );
    }
}
