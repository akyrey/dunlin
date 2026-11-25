//! Reads what the planner needs from the database: the stored `CREATE` texts and two settings.

use dbc_core::DbError;
use dbc_core::dialect::quote_with;
use dbc_core::schema::TableRef;
use rusqlite::{Connection, OptionalExtension, params};

use super::{TableSource, refused};
use crate::errors::map_error;

fn pragma_on(connection: &Connection, name: &str) -> Result<bool, DbError> {
    connection
        .query_row(&format!("PRAGMA {name}"), [], |row| row.get::<_, i64>(0))
        .map(|value| value != 0)
        .map_err(map_error)
}

pub(crate) fn read_source(
    connection: &Connection,
    table: &TableRef,
) -> Result<TableSource, DbError> {
    let schema = table.namespace.schema.as_deref().unwrap_or("main");
    let master = format!("{}.sqlite_master", quote_with(schema, '"'));
    let create_table: Option<String> = connection
        .query_row(
            &format!("SELECT sql FROM {master} WHERE type = 'table' AND name = ?1"),
            params![table.name],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_error)?
        .flatten();
    let create_table =
        create_table.ok_or_else(|| refused(format!("{} has no CREATE TABLE text", table.name)))?;
    let objects = |kind: &str| -> Result<Vec<(String, String)>, DbError> {
        let mut statement = connection
            .prepare(&format!(
                "SELECT name, sql FROM {master} WHERE type = ?1 AND tbl_name = ?2 \
                 AND sql IS NOT NULL ORDER BY rowid"
            ))
            .map_err(map_error)?;
        let rows = statement
            .query_map(params![kind, table.name], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(map_error)?;
        rows.map(|row| row.map_err(map_error)).collect()
    };
    Ok(TableSource {
        create_table,
        indexes: objects("index")?,
        triggers: objects("trigger")?
            .into_iter()
            .map(|(_, sql)| sql)
            .collect(),
        foreign_keys: pragma_on(connection, "foreign_keys")?,
        legacy_alter_table: pragma_on(connection, "legacy_alter_table")?,
    })
}
