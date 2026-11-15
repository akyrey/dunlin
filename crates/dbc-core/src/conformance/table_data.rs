//! Table data conformance (Phase 6): pages with sort, filter, key and offset; counts; cells.

use crate::data::{self, KeyValue, RowCount, SortKey, TablePageRequest};
use crate::dialect::Dialect;
use crate::driver::Connection;
use crate::error::DbErrorCode;
use crate::schema::{NamespaceRef, TableInfo, TableRef};
use crate::value::Value;

use super::{Failure, Harness, ensure, fail};

struct Fixture<'a> {
    connection: &'a dyn Connection,
    dialect: &'a dyn Dialect,
    namespace: NamespaceRef,
}

impl Fixture<'_> {
    async fn info(&self, name: &str) -> Result<TableInfo, Failure> {
        let table = TableRef {
            namespace: self.namespace.clone(),
            name: name.to_owned(),
        };
        self.connection
            .describe_table(&table)
            .await
            .map_err(|e| fail("describe_table", format!("{name}: {e}")))
    }

    fn request(&self, info: &TableInfo) -> TablePageRequest {
        TablePageRequest {
            table: info.table.clone(),
            filter: None,
            key: Vec::new(),
            sort: Vec::new(),
            limit: 100,
            offset: 0,
        }
    }

    /// The first column of every row of the page.
    async fn first_column(
        &self,
        check: &'static str,
        info: &TableInfo,
        request: &TablePageRequest,
    ) -> Result<Vec<Value>, Failure> {
        let page = data::fetch_page(self.connection, self.dialect, info, request)
            .await
            .map_err(|e| fail(check, e.to_string()))?;
        Ok(page
            .rows
            .into_iter()
            .filter_map(|row| row.into_iter().next())
            .collect())
    }
}

fn ints(values: &[i64]) -> Vec<Value> {
    values.iter().map(|v| Value::Int(*v)).collect()
}

fn sort(column: &str, descending: bool) -> SortKey {
    SortKey {
        column: column.to_owned(),
        descending,
    }
}

fn key(column: &str, value: Value) -> KeyValue {
    KeyValue {
        column: column.to_owned(),
        value,
    }
}

/// Pages, counts and cells of the fixture tables through `query_params`.
pub async fn table_data_group(harness: &dyn Harness) -> Result<(), Failure> {
    let driver = harness.driver();
    let config = harness.database(false).await;
    let connection = driver
        .connect(&config)
        .await
        .map_err(|e| fail("connect", e.to_string()))?;
    let dialect = driver.dialect();
    let fixture = Fixture {
        connection: connection.as_ref(),
        dialect: dialect.as_ref(),
        namespace: harness.fixture_namespace(&config),
    };
    pages(&fixture).await?;
    counts(&fixture).await?;
    cells(&fixture).await?;
    connection
        .close()
        .await
        .map_err(|e| fail("close", e.to_string()))
}

async fn pages(fixture: &Fixture<'_>) -> Result<(), Failure> {
    let orders = fixture.info("orders").await?;
    let base = fixture.request(&orders);
    let cases = [
        ("row key order", base.clone(), ints(&[10, 11, 12])),
        (
            "sort and offset",
            TablePageRequest {
                sort: vec![sort("total", true)],
                limit: 2,
                offset: 1,
                ..base.clone()
            },
            ints(&[10, 11]),
        ),
        (
            "multi-column sort",
            TablePageRequest {
                sort: vec![sort("customer_id", true), sort("id", true)],
                ..base.clone()
            },
            ints(&[12, 11, 10]),
        ),
        (
            "filter",
            TablePageRequest {
                filter: Some("total > 1".into()),
                ..base.clone()
            },
            ints(&[10, 12]),
        ),
        (
            "key",
            TablePageRequest {
                key: vec![key("customer_id", Value::Int(1))],
                filter: Some("total >= 0".into()),
                ..base.clone()
            },
            ints(&[10, 11]),
        ),
    ];
    for (check, request, expected) in cases {
        let actual = fixture.first_column(check, &orders, &request).await?;
        ensure(
            actual == expected,
            check,
            format!("expected {expected:?}, got {actual:?}"),
        )?;
    }
    let customers = fixture.info("customers").await?;
    let by_name = TablePageRequest {
        key: vec![key("name", Value::Text("Grace".into()))],
        ..fixture.request(&customers)
    };
    let found = fixture
        .first_column("text key", &customers, &by_name)
        .await?;
    ensure(found == ints(&[2]), "text key", format!("got {found:?}"))?;

    let mixed = fixture.info("Mixed Case \"Quoted\"").await?;
    let quoted = TablePageRequest {
        sort: vec![sort("spaced name", true)],
        key: vec![key("select", Value::Int(1))],
        ..fixture.request(&mixed)
    };
    let found = fixture
        .first_column("quoted identifiers", &mixed, &quoted)
        .await?;
    ensure(
        found == ints(&[1]),
        "quoted identifiers",
        format!("got {found:?}"),
    )?;

    let events = fixture.info("event_log").await?;
    let page = data::fetch_page(
        fixture.connection,
        fixture.dialect,
        &events,
        &fixture.request(&events),
    )
    .await
    .map_err(|e| fail("no row key", e.to_string()))?;
    ensure(
        page.rows.len() == 2,
        "no row key",
        format!("got {} rows", page.rows.len()),
    )?;

    let bad = TablePageRequest {
        filter: Some("total >>> 1".into()),
        ..base
    };
    let code = data::fetch_page(fixture.connection, fixture.dialect, &orders, &bad)
        .await
        .err()
        .map(|e| e.code);
    ensure(
        code == Some(DbErrorCode::Syntax),
        "bad filter",
        format!("got {code:?}"),
    )
}

async fn counts(fixture: &Fixture<'_>) -> Result<(), Failure> {
    let orders = fixture.info("orders").await?;
    let count = |filter: Option<&'static str>, keys: Vec<KeyValue>, exact: bool| {
        let orders = orders.clone();
        async move {
            data::count_rows(
                fixture.connection,
                fixture.dialect,
                &orders,
                filter,
                &keys,
                exact,
            )
            .await
            .map_err(|e| fail("count_rows", e.to_string()))
        }
    };
    let all = count(None, Vec::new(), true).await?;
    let filtered = count(Some("total > 1"), Vec::new(), true).await?;
    let keyed = count(None, vec![key("customer_id", Value::Int(1))], true).await?;
    let estimate = count(None, Vec::new(), false).await?;
    let filtered_estimate = count(Some("total > 1"), Vec::new(), false).await?;
    ensure(
        all == RowCount::Exact(3),
        "exact count",
        format!("got {all:?}"),
    )?;
    ensure(
        filtered == RowCount::Exact(2),
        "filtered count",
        format!("got {filtered:?}"),
    )?;
    ensure(
        keyed == RowCount::Exact(2),
        "keyed count",
        format!("got {keyed:?}"),
    )?;
    ensure(
        matches!(estimate, RowCount::Estimate(_) | RowCount::Unknown),
        "estimate",
        format!("got {estimate:?}"),
    )?;
    ensure(
        filtered_estimate == RowCount::Unknown,
        "filtered estimate",
        format!("got {filtered_estimate:?}"),
    )
}

async fn cells(fixture: &Fixture<'_>) -> Result<(), Failure> {
    let customers = fixture.info("customers").await?;
    let email = data::fetch_cell(
        fixture.connection,
        fixture.dialect,
        &customers,
        &[key("id", Value::Int(1))],
        "email",
    )
    .await
    .map_err(|e| fail("cell", e.to_string()))?;
    ensure(
        email == Value::Text("ada@example.com".into()),
        "cell",
        format!("got {email:?}"),
    )?;

    let items = fixture.info("order_items").await?;
    let sku = data::fetch_cell(
        fixture.connection,
        fixture.dialect,
        &items,
        &[key("line", Value::Int(2)), key("order_id", Value::Int(10))],
        "sku",
    )
    .await
    .map_err(|e| fail("composite key cell", e.to_string()))?;
    ensure(
        sku == Value::Text("B-7".into()),
        "composite key cell",
        format!("got {sku:?}"),
    )?;

    let missing = data::fetch_cell(
        fixture.connection,
        fixture.dialect,
        &customers,
        &[key("id", Value::Int(999))],
        "email",
    )
    .await
    .err()
    .map(|e| e.code);
    ensure(
        missing == Some(DbErrorCode::NotFound),
        "missing row",
        format!("got {missing:?}"),
    )?;

    let events = fixture.info("event_log").await?;
    let unaddressable = data::fetch_cell(
        fixture.connection,
        fixture.dialect,
        &events,
        &[key("message", Value::Text("started".into()))],
        "message",
    )
    .await
    .err()
    .map(|e| e.code);
    ensure(
        unaddressable == Some(DbErrorCode::Unsupported),
        "no row key",
        format!("got {unaddressable:?}"),
    )
}

/// Requests every dialect snapshots through `data::page_sql` (task 6.2): no filter, filter,
/// multi-column sort, offset, identifiers needing quotes, a key, and a table without a row key.
pub fn page_sql_cases(
    namespace: &NamespaceRef,
) -> Vec<(&'static str, TableInfo, TablePageRequest)> {
    use crate::result::TypeCategory;
    use crate::schema::{Column, Key, ObjectKind};

    let column = |name: &str, type_name: &str| Column {
        name: name.into(),
        type_name: type_name.into(),
        category: TypeCategory::Integer,
        nullable: false,
        default: None,
        comment: None,
        ordinal: 1,
        generated: false,
    };
    let table = |name: &str, row_key: Option<Vec<String>>| TableInfo {
        table: TableRef {
            namespace: namespace.clone(),
            name: name.into(),
        },
        kind: ObjectKind::Table,
        comment: None,
        columns: vec![
            column("id", "integer"),
            column("select", "integer"),
            column("spaced \"name\"", "text"),
        ],
        primary_key: row_key.clone().map(|columns| Key {
            name: None,
            columns,
        }),
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
        unique_constraints: Vec::new(),
        check_constraints: Vec::new(),
        row_key,
        row_id: None,
    };
    let keyed = table("Order Items", Some(vec!["id".into(), "select".into()]));
    let unkeyed = table("event_log", None);
    let request = |info: &TableInfo| TablePageRequest {
        table: info.table.clone(),
        filter: None,
        key: Vec::new(),
        sort: Vec::new(),
        limit: 200,
        offset: 0,
    };
    vec![
        ("plain", keyed.clone(), request(&keyed)),
        (
            "filter",
            keyed.clone(),
            TablePageRequest {
                filter: Some("\"select\" > 1 OR id IS NULL".into()),
                ..request(&keyed)
            },
        ),
        (
            "sort_and_offset",
            keyed.clone(),
            TablePageRequest {
                sort: vec![sort("spaced \"name\"", true), sort("id", false)],
                limit: 50,
                offset: 150,
                ..request(&keyed)
            },
        ),
        (
            "key",
            keyed.clone(),
            TablePageRequest {
                key: vec![
                    key("select", Value::Int(7)),
                    key("spaced \"name\"", Value::Null),
                ],
                filter: Some("id > 0".into()),
                ..request(&keyed)
            },
        ),
        ("no_row_key", unkeyed.clone(), request(&unkeyed)),
    ]
}
