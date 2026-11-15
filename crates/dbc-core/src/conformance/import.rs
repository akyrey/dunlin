//! CSV import conformance (D-034): typed values, defaults for unmapped columns, empty text
//! versus `NULL`, and a refused row rolling the whole import back.

use tokio::sync::mpsc;

use crate::data::{self, KeyValue, TablePageRequest};
use crate::dialect::Dialect;
use crate::driver::Connection;
use crate::export::CsvOptions;
use crate::import::{self, ImportError, ImportTarget};
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

    /// Imports `csv` (with a header) into `info`, mapping each named column to the CSV field
    /// of the same name.
    async fn import(
        &self,
        info: &TableInfo,
        csv: &str,
        columns: &[&str],
    ) -> Result<u64, ImportError> {
        let options = CsvOptions::default();
        let header: Vec<&str> = csv.lines().next().unwrap_or_default().split(',').collect();
        let target = ImportTarget {
            table: info.table.clone(),
            columns: columns
                .iter()
                .filter_map(|name| {
                    let column = info.columns.iter().find(|c| c.name == *name)?.clone();
                    let source = header.iter().position(|h| h == name)?;
                    Some((column, source))
                })
                .collect(),
        };
        let (tx, rx) = mpsc::channel(4);
        // Small batches, so a failure in a later batch must undo the earlier ones.
        import::read_batches(csv.as_bytes(), &options, header.len(), 1, |batch| {
            tx.try_send(Ok(batch)).is_ok()
        })?;
        drop(tx);
        import::import(
            self.connection,
            self.dialect,
            &target,
            &options.null,
            rx,
            &|_| {},
        )
        .await
    }

    async fn row(&self, info: &TableInfo, id: i64) -> Result<Option<Vec<Value>>, Failure> {
        let request = TablePageRequest {
            table: info.table.clone(),
            filter: None,
            key: vec![KeyValue {
                column: "id".into(),
                value: Value::Int(id),
            }],
            sort: Vec::new(),
            limit: 1,
            offset: 0,
            after: Vec::new(),
        };
        let page = data::fetch_page(self.connection, self.dialect, info, &request)
            .await
            .map_err(|e| fail("read back", e.to_string()))?;
        Ok(page.rows.into_iter().next())
    }

    fn column(info: &TableInfo, row: &[Value], name: &str) -> Value {
        info.columns
            .iter()
            .position(|c| c.name == name)
            .and_then(|i| row.get(i).cloned())
            .unwrap_or(Value::Null)
    }
}

/// Imports into the fixture tables and reads the rows back.
pub async fn import_group(harness: &dyn Harness) -> Result<(), Failure> {
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
    customers(&fixture).await?;
    typed(&fixture).await?;
    connection
        .close()
        .await
        .map_err(|e| fail("close", e.to_string()))
}

async fn customers(fixture: &Fixture<'_>) -> Result<(), Failure> {
    let check = "import customers";
    let info = fixture.info("customers").await?;
    let csv = "name,id,email\nZed,100,\n\"\",101,\"\"\n";
    let inserted = fixture
        .import(&info, csv, &["id", "name", "email"])
        .await
        .map_err(|e| fail(check, e.to_string()))?;
    ensure(inserted == 2, check, format!("inserted {inserted}"))?;
    let zed = fixture
        .row(&info, 100)
        .await?
        .ok_or_else(|| fail(check, "row 100 missing"))?;
    let empty = fixture
        .row(&info, 101)
        .await?
        .ok_or_else(|| fail(check, "row 101 missing"))?;
    ensure(
        Fixture::column(&info, &zed, "email") == Value::Null,
        check,
        "an empty field is NULL",
    )?;
    ensure(
        Fixture::column(&info, &empty, "name") == Value::Text(String::new()),
        check,
        format!(
            "a quoted empty field is empty text, got {:?}",
            Fixture::column(&info, &empty, "name")
        ),
    )?;
    ensure(
        Fixture::column(&info, &zed, "created_at") != Value::Null,
        check,
        "an unmapped column takes its default",
    )?;

    // Row 102 is fine, but 100 exists: the whole import, 102 included, rolls back.
    let rolled = "check import rollback";
    let duplicate = fixture
        .import(&info, "name,id\nNew,102\nAgain,100\n", &["id", "name"])
        .await;
    ensure(
        matches!(
            duplicate,
            Err(ImportError::Rows {
                first: 3,
                last: 3,
                ..
            })
        ),
        rolled,
        format!("got {duplicate:?}"),
    )?;
    ensure(
        fixture.row(&info, 102).await?.is_none(),
        rolled,
        "row 102 was kept",
    )?;

    let bad = fixture
        .import(&info, "name,id\nOne,x1\n", &["id", "name"])
        .await;
    ensure(
        matches!(bad, Err(ImportError::Value { line: 2, .. })),
        "import bad value",
        format!("got {bad:?}"),
    )
}

async fn typed(fixture: &Fixture<'_>) -> Result<(), Failure> {
    let check = "import typed values";
    let info = fixture.info("all_types").await?;
    let csv = "id,c_bool,c_int,c_bigint,c_real,c_decimal,c_text,c_blob,c_date\n\
               300,true,-7,9007199254740993,2.5,12345678901234.5,\"a,\"\"b\"\"\",\\x00ff,2026-01-02\n";
    let columns = [
        "id",
        "c_bool",
        "c_int",
        "c_bigint",
        "c_real",
        "c_decimal",
        "c_text",
        "c_blob",
        "c_date",
    ];
    fixture
        .import(&info, csv, &columns)
        .await
        .map_err(|e| fail(check, e.to_string()))?;
    let row = fixture
        .row(&info, 300)
        .await?
        .ok_or_else(|| fail(check, "row 300 missing"))?;
    let expect = [
        ("c_int", Value::Int(-7)),
        ("c_bigint", Value::Int(9_007_199_254_740_993)),
        ("c_real", Value::Float(2.5)),
        ("c_text", Value::Text("a,\"b\"".into())),
        ("c_blob", Value::Bytes(vec![0, 255])),
    ];
    for (name, expected) in expect {
        let actual = Fixture::column(&info, &row, name);
        ensure(
            actual == expected,
            check,
            format!("{name}: expected {expected:?}, got {actual:?}"),
        )?;
    }
    let flag = Fixture::column(&info, &row, "c_bool");
    ensure(
        matches!(flag, Value::Bool(true) | Value::Int(1)),
        check,
        format!("c_bool: got {flag:?}"),
    )?;
    let decimal = Fixture::column(&info, &row, "c_decimal");
    let text = match &decimal {
        Value::Decimal(t) | Value::Text(t) => t.clone(),
        Value::Float(f) => f.to_string(),
        other => format!("{other:?}"),
    };
    ensure(
        text.starts_with("12345678901234.5"),
        check,
        format!("c_decimal: got {decimal:?}"),
    )
}
