//! Table statistics from the catalogs and the cumulative statistics views (Phase 10).

use dbc_core::DbError;
use dbc_core::schema::TableRef;
use dbc_core::stats::{IndexStats, QUERY_STATS_LIMIT, QueryStat, QueryStats, TableStats, count};
use tokio_postgres::Client;

use crate::introspect::{self, get, query};

/// Sizes and row estimate from `pg_class`; vacuum, analyze and dead rows from
/// `pg_stat_user_tables`; per-index size and scans from `pg_stat_user_indexes`.
pub(crate) async fn table_stats(client: &Client, table: &TableRef) -> Result<TableStats, DbError> {
    let relation = introspect::relation(client, table).await?;
    let rows = query(
        client,
        "SELECT pg_total_relation_size(c.oid), pg_relation_size(c.oid), pg_indexes_size(c.oid), \
                c.reltuples::bigint, s.n_dead_tup, \
                greatest(s.last_vacuum, s.last_autovacuum)::text, \
                greatest(s.last_analyze, s.last_autoanalyze)::text, \
                (SELECT stats_reset::text FROM pg_stat_database WHERE datname = current_database()) \
         FROM pg_class c LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid \
         WHERE c.oid = $1",
        &[&relation.oid],
    )
    .await?;
    let Some(row) = rows.first() else {
        return Ok(TableStats::default());
    };
    Ok(TableStats {
        total_bytes: count(get(row, 0)?),
        table_bytes: count(get(row, 1)?),
        index_bytes: count(get(row, 2)?),
        estimated_rows: count(get(row, 3)?),
        dead_rows: count(get(row, 4)?),
        last_vacuum: get(row, 5)?,
        last_analyze: get(row, 6)?,
        counted_since: get(row, 7)?,
        updated_at: None,
        indexes: indexes(client, relation.oid).await?,
    })
}

async fn indexes(
    client: &Client,
    oid: tokio_postgres::types::Oid,
) -> Result<Vec<IndexStats>, DbError> {
    let rows = query(
        client,
        "SELECT i.relname::text, pg_relation_size(i.oid), s.idx_scan \
         FROM pg_index x JOIN pg_class i ON i.oid = x.indexrelid \
         LEFT JOIN pg_stat_user_indexes s ON s.indexrelid = x.indexrelid \
         WHERE x.indrelid = $1 ORDER BY 1",
        &[&oid],
    )
    .await?;
    rows.iter()
        .map(|row| {
            Ok(IndexStats {
                name: get(row, 0)?,
                bytes: count(get(row, 1)?),
                scans: count(get(row, 2)?),
            })
        })
        .collect()
}

const NOT_INSTALLED: &str = "pg_stat_statements is not installed in this database. Add it to \
    shared_preload_libraries in postgresql.conf, restart the server, then run \
    CREATE EXTENSION pg_stat_statements.";

/// The heaviest statements of the current database from `pg_stat_statements`, or how to enable
/// it. A server that refuses to read it (not preloaded, no privilege) says why.
pub(crate) async fn query_stats(client: &Client) -> Result<QueryStats, DbError> {
    let installed = query(
        client,
        "SELECT 1 FROM pg_extension WHERE extname = 'pg_stat_statements'",
        &[],
    )
    .await?;
    if installed.is_empty() {
        return Ok(QueryStats::Unavailable {
            reason: NOT_INSTALLED.into(),
        });
    }
    let limit = i64::try_from(QUERY_STATS_LIMIT).unwrap_or(50);
    let found = query(
        client,
        "SELECT query, calls, total_exec_time, mean_exec_time, rows FROM pg_stat_statements \
         WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
         ORDER BY total_exec_time DESC LIMIT $1",
        &[&limit],
    )
    .await;
    let rows = match found {
        Ok(rows) => rows,
        Err(error) => {
            return Ok(QueryStats::Unavailable {
                reason: error.message,
            });
        }
    };
    let statements = rows
        .iter()
        .map(|row| {
            Ok(QueryStat {
                statement: get::<Option<String>>(row, 0)?.unwrap_or_default(),
                calls: count(get(row, 1)?).unwrap_or(0),
                total_ms: get(row, 2)?,
                mean_ms: get(row, 3)?,
                rows: count(get(row, 4)?),
            })
        })
        .collect::<Result<_, DbError>>()?;
    Ok(QueryStats::Available { statements })
}
