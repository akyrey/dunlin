//! Conformance group "explain analyze" (task 10.12, D-043): measured plans, writes undone (or
//! refused where the driver cannot undo them), hints on a real full scan, and the statistics.

use std::sync::Arc;

use super::{Failure, Harness, ensure, fail};
use crate::driver::Connection;
use crate::plan::{HintKind, hints};
use crate::result::{ExecuteOptions, StatementOutcome};
use crate::schema::TableRef;
use crate::stats::QueryStats;

/// Rows in the table whose full scan must be flagged: above `plan::FULL_SCAN_ROWS`.
const SCAN_ROWS: usize = 20_000;

async fn run(
    connection: &dyn Connection,
    check: &'static str,
    sql: &str,
) -> Result<StatementOutcome, Failure> {
    connection
        .execute(sql, &ExecuteOptions::default())
        .await
        .map_err(|e| fail(check, format!("{sql}: {e}")))
}

async fn row_count(connection: &dyn Connection, table: &str) -> Result<String, Failure> {
    let outcome = run(
        connection,
        "count",
        &format!("SELECT count(*) FROM {table}"),
    )
    .await?;
    Ok(format!("{:?}", crate::plan::column_text(&outcome, 0)))
}

pub async fn explain_analyze_group(harness: &dyn Harness) -> Result<(), Failure> {
    let driver = harness.driver();
    let capabilities = driver.info().capabilities;
    let config = harness.database(false).await;
    let connection: Arc<dyn Connection> = driver
        .connect(&config)
        .await
        .map_err(|e| fail("connect", e.to_string()))?;
    let dialect = driver.dialect();
    let namespace = harness.fixture_namespace(&config);
    let qualified = |name: &str| {
        dialect.qualified_name(&TableRef {
            namespace: namespace.clone(),
            name: name.into(),
        })
    };
    if capabilities.explain_analyze {
        measured_read(connection.as_ref(), &qualified("orders")).await?;
        undone_write(
            connection.as_ref(),
            &qualified("event_log"),
            capabilities.explain_analyze_writes,
        )
        .await?;
        if harness.plans_count_rows() {
            full_scan_hint(
                connection.as_ref(),
                &qualified("ea_scan"),
                &qualified("ea_digits"),
                dialect.splitter_rules(),
            )
            .await?;
        }
    }
    if capabilities.table_stats {
        let table = TableRef {
            namespace: namespace.clone(),
            name: "orders".into(),
        };
        let stats = connection
            .table_stats(&table)
            .await
            .map_err(|e| fail("table_stats", e.to_string()))?;
        ensure(
            !stats.indexes.is_empty(),
            "table_stats",
            format!("no indexes: {stats:?}"),
        )?;
    }
    if capabilities.query_stats {
        let stats = connection
            .query_stats()
            .await
            .map_err(|e| fail("query_stats", e.to_string()))?;
        if let QueryStats::Unavailable { reason } = &stats {
            ensure(!reason.is_empty(), "query_stats", "an empty reason")?;
        }
    }
    Ok(())
}

async fn measured_read(connection: &dyn Connection, orders: &str) -> Result<(), Failure> {
    let analyzed = connection
        .explain_analyze(&format!("SELECT * FROM {orders} WHERE total > 0"), false)
        .await
        .map_err(|e| fail("explain analyze read", e.to_string()))?;
    ensure(
        analyzed.total_ms.is_some_and(|ms| ms >= 0.0),
        "explain analyze read",
        format!("no total time: {analyzed:?}"),
    )
}

async fn undone_write(
    connection: &dyn Connection,
    events: &str,
    can_undo: bool,
) -> Result<(), Failure> {
    let before = row_count(connection, events).await?;
    let outcome = connection
        .explain_analyze(&format!("DELETE FROM {events}"), true)
        .await;
    match (can_undo, outcome) {
        (true, Ok(_)) => {}
        (true, Err(e)) => return Err(fail("explain analyze write", e.to_string())),
        (false, Ok(_)) => {
            return Err(fail(
                "explain analyze write",
                "a write ran where it cannot be undone",
            ));
        }
        (false, Err(e)) => ensure(
            e.code == crate::DbErrorCode::Unsupported,
            "explain analyze write",
            format!("expected Unsupported, got {e:?}"),
        )?,
    }
    let after = row_count(connection, events).await?;
    ensure(
        before == after,
        "explain analyze write",
        format!("rows changed: {before} → {after}"),
    )
}

/// A table of `SCAN_ROWS` rows filtered on an unindexed column: the scan is flagged and an
/// index on that column is suggested.
async fn full_scan_hint(
    connection: &dyn Connection,
    table: &str,
    digits: &str,
    rules: crate::sql::SplitterRules,
) -> Result<(), Failure> {
    for sql in [
        format!("CREATE TABLE {digits} (d integer NOT NULL)"),
        format!("INSERT INTO {digits} (d) VALUES (0), (1), (2), (3), (4), (5), (6), (7), (8), (9)"),
        format!("CREATE TABLE {table} (id integer NOT NULL PRIMARY KEY, v integer)"),
        format!(
            "INSERT INTO {table} (id, v) SELECT a.d + 10 * b.d + 100 * c.d + 1000 * e.d + 10000 * f.d, \
             a.d + 10 * b.d \
             FROM {digits} a, {digits} b, {digits} c, {digits} e, {digits} f \
             WHERE a.d + 10 * b.d + 100 * c.d + 1000 * e.d + 10000 * f.d < {SCAN_ROWS}"
        ),
    ] {
        run(connection, "full scan setup", &sql).await?;
    }
    let analyzed = connection
        .explain_analyze(&format!("SELECT * FROM {table} WHERE v = 5"), false)
        .await
        .map_err(|e| fail("full scan hint", e.to_string()))?;
    let found = hints(&analyzed.plan, rules);
    let scan = found
        .iter()
        .find(|h| h.kind == HintKind::FullScan)
        .ok_or_else(|| {
            fail(
                "full scan hint",
                format!("no full scan hint in {found:?} for {:#?}", analyzed.plan),
            )
        })?;
    let column = scan.suggestion.as_ref().map(|s| s.column.as_str());
    ensure(
        column == Some("v"),
        "full scan hint",
        format!("expected an index on v, got {scan:?}"),
    )
}
