//! Table statistics (Phase 10). SQLite keeps no row estimates or usage counts; sizes come from
//! the `dbstat` table when the library was built with it, and are unknown otherwise.

use dbc_core::DbError;
use dbc_core::dialect::quote_with;
use dbc_core::schema::TableRef;
use dbc_core::stats::{IndexStats, TableStats};
use rusqlite::Connection;

use crate::errors::map_error;

pub(crate) fn table_stats(
    connection: &Connection,
    table: &TableRef,
) -> Result<TableStats, DbError> {
    let schema = table.namespace.schema.as_deref().unwrap_or("main");
    let list = format!(
        "SELECT name FROM {}.pragma_index_list(?1) ORDER BY name",
        quote_with(schema, '"')
    );
    let mut statement = connection.prepare(&list).map_err(map_error)?;
    let names = statement
        .query_map([&table.name], |row| row.get::<_, String>(0))
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)?;
    let size = |name: &str| bytes(connection, schema, name);
    let indexes: Vec<IndexStats> = names
        .into_iter()
        .map(|name| IndexStats {
            bytes: size(&name),
            name,
            scans: None,
        })
        .collect();
    let table_bytes = size(&table.name);
    let index_bytes: Option<u64> = indexes.iter().map(|i| i.bytes).sum();
    Ok(TableStats {
        total_bytes: table_bytes.zip(index_bytes).map(|(t, i)| t + i),
        table_bytes,
        index_bytes,
        indexes,
        ..TableStats::default()
    })
}

/// Bytes of one table or index from `dbstat`; `None` without it.
fn bytes(connection: &Connection, schema: &str, name: &str) -> Option<u64> {
    connection
        .query_row(
            "SELECT sum(pgsize) FROM dbstat(?1) WHERE name = ?2",
            [schema, name],
            |row| row.get::<_, Option<i64>>(0),
        )
        .ok()
        .flatten()
        .and_then(|b| u64::try_from(b).ok())
}
