//! Table and column names of one database in a single query, for editor autocomplete.

use dbc_core::DbError;
use dbc_core::schema::{CompletionSchema, CompletionTable, NamespaceRef};
use mysql_async::Conn;
use mysql_async::prelude::Queryable;

use crate::errors::map_error;

pub(crate) async fn completion_schema(
    conn: &mut Conn,
    namespace: &NamespaceRef,
) -> Result<CompletionSchema, DbError> {
    let schema = namespace.schema.clone().unwrap_or_default();
    let rows: Vec<(String, String)> = conn
        .exec(
            "SELECT TABLE_NAME, COLUMN_NAME FROM information_schema.COLUMNS \
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, ORDINAL_POSITION",
            (schema.clone(),),
        )
        .await
        .map_err(|e| map_error(&e))?;
    let mut tables: Vec<CompletionTable> = Vec::new();
    for (table, column) in rows {
        if tables.last().is_none_or(|t| t.name != table) {
            tables.push(CompletionTable {
                schema: Some(schema.clone()),
                name: table,
                columns: Vec::new(),
            });
        }
        if let Some(last) = tables.last_mut() {
            last.columns.push(column);
        }
    }
    Ok(CompletionSchema { tables })
}
