//! Table and column names of one schema in a single catalog query, for editor autocomplete.

use dbc_core::DbError;
use dbc_core::schema::{CompletionSchema, CompletionTable, NamespaceRef};
use tokio_postgres::Client;

use crate::introspect::{get, query};

const COMPLETION: &str = "\
    SELECT c.relname::text, \
           array_agg(a.attname::text ORDER BY a.attnum) FILTER (WHERE a.attname IS NOT NULL) \
    FROM pg_class c \
    JOIN pg_namespace n ON n.oid = c.relnamespace \
    LEFT JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped \
    WHERE n.nspname = $1 AND c.relkind IN ('r', 'p', 'f', 'v', 'm') AND NOT c.relispartition \
    GROUP BY c.relname ORDER BY c.relname";

pub(crate) async fn completion_schema(
    client: &Client,
    namespace: &NamespaceRef,
) -> Result<CompletionSchema, DbError> {
    let schema = namespace.schema.as_deref().unwrap_or("public");
    let rows = query(client, COMPLETION, &[&schema]).await?;
    let tables = rows
        .iter()
        .map(|row| {
            Ok(CompletionTable {
                schema: Some(schema.to_owned()),
                name: get(row, 0)?,
                columns: get::<Option<Vec<String>>>(row, 1)?.unwrap_or_default(),
            })
        })
        .collect::<Result<_, DbError>>()?;
    Ok(CompletionSchema { tables })
}
