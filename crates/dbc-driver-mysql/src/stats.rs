//! Table statistics (Phase 10): sizes and row estimate from `information_schema.tables`, index
//! sizes from `mysql.innodb_index_stats` and index use from `performance_schema`. The last two
//! need privileges or `performance_schema`; without them those numbers are unknown, not errors.

use std::collections::HashMap;

use dbc_core::schema::TableRef;
use dbc_core::stats::{IndexStats, TableStats};
use dbc_core::{DbError, DbErrorCode};
use mysql_async::Conn;

use crate::introspect::{params, rows, text};

fn number(value: Option<String>) -> Option<u64> {
    value
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v.max(0.0) as u64)
}

pub(crate) async fn table_stats(conn: &mut Conn, table: &TableRef) -> Result<TableStats, DbError> {
    let found = rows(
        conn,
        "SELECT data_length, index_length, table_rows, CAST(update_time AS CHAR) \
         FROM information_schema.tables WHERE table_schema = ? AND table_name = ?",
        params(table),
    )
    .await?;
    let row = found.first().ok_or_else(|| {
        DbError::new(
            DbErrorCode::NotFound,
            format!("{} does not exist", table.name),
        )
    })?;
    let (data, index) = (number(text(row, 0)), number(text(row, 1)));
    let names = rows(
        conn,
        "SELECT DISTINCT index_name FROM information_schema.statistics \
         WHERE table_schema = ? AND table_name = ? ORDER BY index_name",
        params(table),
    )
    .await?;
    // Optional sources: a refusal (privileges, performance_schema off) leaves them unknown.
    let sizes = pairs(
        conn,
        "SELECT index_name, stat_value * @@innodb_page_size FROM mysql.innodb_index_stats \
         WHERE database_name = ? AND table_name = ? AND stat_name = 'size'",
        table,
    )
    .await;
    let scans = pairs(
        conn,
        "SELECT index_name, count_star \
         FROM performance_schema.table_io_waits_summary_by_index_usage \
         WHERE object_schema = ? AND object_name = ? AND index_name IS NOT NULL",
        table,
    )
    .await;
    let counted_since = if scans.is_empty() {
        None
    } else {
        server_start(conn).await
    };
    Ok(TableStats {
        total_bytes: data.zip(index).map(|(d, i)| d + i),
        table_bytes: data,
        index_bytes: index,
        estimated_rows: number(text(row, 2)),
        updated_at: text(row, 3),
        counted_since,
        indexes: names
            .iter()
            .filter_map(|r| text(r, 0))
            .map(|name| IndexStats {
                bytes: sizes.get(&name).copied(),
                scans: scans.get(&name).copied(),
                name,
            })
            .collect(),
        ..TableStats::default()
    })
}

async fn pairs(conn: &mut Conn, sql: &str, table: &TableRef) -> HashMap<String, u64> {
    let Ok(found) = rows(conn, sql, params(table)).await else {
        return HashMap::new();
    };
    found
        .iter()
        .filter_map(|row| Some((text(row, 0)?, number(text(row, 1))?)))
        .collect()
}

/// Index use is counted since the server started.
async fn server_start(conn: &mut Conn) -> Option<String> {
    let found = rows(
        conn,
        "SELECT CAST(NOW() - INTERVAL VARIABLE_VALUE SECOND AS CHAR) \
         FROM performance_schema.global_status WHERE VARIABLE_NAME = 'Uptime'",
        Vec::new(),
    )
    .await
    .ok()?;
    found.first().and_then(|row| text(row, 0))
}
