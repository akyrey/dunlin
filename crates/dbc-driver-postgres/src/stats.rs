//! Table statistics from the catalogs and the cumulative statistics views (Phase 10).

use dbc_core::DbError;
use dbc_core::schema::TableRef;
use dbc_core::stats::{IndexStats, TableStats, count};
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
