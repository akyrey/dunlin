//! Table and column names of one schema in a single query, for editor autocomplete.

use dbc_core::DbError;
use dbc_core::schema::{CompletionSchema, CompletionTable, NamespaceRef};
use rusqlite::Connection;

use crate::errors::map_error;

const COMPLETION: &str = "\
    SELECT t.name, c.name FROM pragma_table_list AS t \
    JOIN pragma_table_xinfo(t.name, t.schema) AS c \
    WHERE t.schema = ?1 AND t.type IN ('table', 'view', 'virtual') \
      AND t.name NOT LIKE 'sqlite\\_%' ESCAPE '\\' AND c.hidden <> 1 \
    ORDER BY t.name, c.cid";

pub(crate) fn completion_schema(
    connection: &Connection,
    namespace: &NamespaceRef,
) -> Result<CompletionSchema, DbError> {
    let schema = namespace
        .schema
        .clone()
        .unwrap_or_else(|| "main".to_owned());
    let mut statement = connection.prepare(COMPLETION).map_err(map_error)?;
    let rows = statement
        .query_map([&schema], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(map_error)?;
    let mut tables: Vec<CompletionTable> = Vec::new();
    for row in rows {
        let (table, column) = row.map_err(map_error)?;
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
