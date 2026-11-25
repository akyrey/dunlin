//! Schema edit plans (task 9.7, D-042). SQLite alters a table natively only to add, rename and
//! drop columns and to rename it; anything else rebuilds the table ([`rebuild`]). Both kinds run
//! in one transaction.

mod matching;
mod rebuild;
mod source;

use dbc_core::dialect::Dialect;
use dbc_core::schema::{Index, TableInfo, TableRef};
use dbc_core::schema_edit::render::{default_clause, index_parts};
use dbc_core::schema_edit::{ColumnDefinition, SchemaChange, SchemaEdit, SchemaPlan, table_states};
use dbc_core::{DbError, DbErrorCode};

use crate::SqliteDialect;

pub(crate) use source::read_source;

/// What the planner reads from the database besides the edit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableSource {
    /// The table's `CREATE TABLE` text from `sqlite_master`.
    pub create_table: String,
    /// `CREATE INDEX` texts of the table's explicit indexes, by name (automatic indexes that
    /// back keys have no text and are left out).
    pub indexes: Vec<(String, String)>,
    /// `CREATE TRIGGER` texts of the table's triggers.
    pub triggers: Vec<String>,
    /// `PRAGMA foreign_keys` on this connection.
    pub foreign_keys: bool,
    /// `PRAGMA legacy_alter_table` on this connection.
    pub legacy_alter_table: bool,
}

pub(crate) fn refused(message: impl Into<String>) -> DbError {
    DbError::new(DbErrorCode::Execution, message)
}

pub(crate) fn ident(name: &str) -> String {
    SqliteDialect.quote_identifier(name)
}

/// The statements that make `edit` on the table `source` describes.
pub fn plan_schema_edit(edit: &SchemaEdit, source: &TableSource) -> Result<SchemaPlan, DbError> {
    let states = table_states(&SqliteDialect, edit)?;
    for change in &edit.changes {
        refuse_unsupported(&states[0], change)?;
    }
    let destructive = edit.changes.iter().any(SchemaChange::is_destructive);
    if edit
        .changes
        .iter()
        .zip(&states)
        .all(|(change, before)| is_native(before, change))
    {
        let statements = edit
            .changes
            .iter()
            .zip(&states)
            .map(|(change, before)| native_statement(before, change))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(SchemaPlan {
            statements,
            transactional: true,
            destructive,
            ..SchemaPlan::default()
        });
    }
    rebuild::plan(edit, &states, source)
}

/// SQLite has no comments and no index methods.
fn refuse_unsupported(original: &TableInfo, change: &SchemaChange) -> Result<(), DbError> {
    let commented = match change {
        SchemaChange::SetComment { .. } => true,
        SchemaChange::AddColumn { column } => column.comment.is_some(),
        SchemaChange::AlterColumn { name, column, .. } => {
            let old = original.column_named(name).and_then(|c| c.comment);
            column.comment.is_some() && column.comment != old
        }
        _ => false,
    };
    if commented {
        return Err(refused("SQLite has no table or column comments"));
    }
    if let SchemaChange::AddIndex { index } = change
        && index.method.is_some()
    {
        return Err(refused("SQLite has no index methods"));
    }
    Ok(())
}

/// A default SQLite's `ADD COLUMN` accepts: not an expression and not the current time.
fn native_default(default: &str) -> bool {
    let upper = default.trim().to_ascii_uppercase();
    !upper.starts_with('(') && !upper.starts_with("CURRENT_")
}

/// A column change that only renames.
fn only_renames(before: &TableInfo, name: &str, column: &ColumnDefinition) -> bool {
    before.column_named(name).is_some_and(|old| {
        old.type_name == column.type_name
            && old.nullable == column.nullable
            && old.default == column.default
    })
}

fn is_native(before: &TableInfo, change: &SchemaChange) -> bool {
    match change {
        SchemaChange::AddColumn { column } => match &column.default {
            Some(default) => native_default(default),
            None => column.nullable,
        },
        SchemaChange::AlterColumn { name, column, .. } => only_renames(before, name, column),
        SchemaChange::DropColumn { .. }
        | SchemaChange::AddIndex { .. }
        | SchemaChange::DropIndex { .. }
        | SchemaChange::RenameTable { .. } => true,
        _ => false,
    }
}

/// `"name" type [NOT NULL] [DEFAULT x]`
pub(crate) fn column_sql(column: &ColumnDefinition) -> String {
    let mut sql = ident(&column.name);
    if !column.type_name.is_empty() {
        sql.push(' ');
        sql.push_str(&column.type_name);
    }
    if !column.nullable {
        sql.push_str(" NOT NULL");
    }
    if let Some(default) = &column.default {
        sql.push(' ');
        sql.push_str(&default_clause(default));
    }
    sql
}

/// `CREATE [UNIQUE] INDEX "schema"."name" ON "table" (...) [WHERE ...]`: SQLite qualifies the
/// index, not the table.
pub(crate) fn create_index(info: &TableInfo, table_name: &str, index: &Index) -> String {
    let qualified = SqliteDialect.qualified_name(&TableRef {
        namespace: info.table.namespace.clone(),
        name: index.name.clone(),
    });
    let mut sql = format!(
        "CREATE {}INDEX {qualified} ON {} ({})",
        if index.unique { "UNIQUE " } else { "" },
        ident(table_name),
        index_parts(&SqliteDialect, info, &index.columns)
    );
    if let Some(predicate) = &index.predicate {
        sql.push_str(&format!(" WHERE {predicate}"));
    }
    sql
}

pub(crate) fn drop_index(info: &TableInfo, name: &str) -> String {
    let qualified = SqliteDialect.qualified_name(&TableRef {
        namespace: info.table.namespace.clone(),
        name: name.to_owned(),
    });
    format!("DROP INDEX {qualified}")
}

fn native_statement(before: &TableInfo, change: &SchemaChange) -> Result<String, DbError> {
    let table = SqliteDialect.qualified_name(&before.table);
    Ok(match change {
        SchemaChange::AddColumn { column } => {
            format!("ALTER TABLE {table} ADD COLUMN {}", column_sql(column))
        }
        SchemaChange::AlterColumn { name, column, .. } => format!(
            "ALTER TABLE {table} RENAME COLUMN {} TO {}",
            ident(name),
            ident(&column.name)
        ),
        SchemaChange::DropColumn { name } => {
            format!("ALTER TABLE {table} DROP COLUMN {}", ident(name))
        }
        SchemaChange::AddIndex { index } => create_index(before, &before.table.name, index),
        SchemaChange::DropIndex { index } => drop_index(before, &index.name),
        SchemaChange::RenameTable { name } => {
            format!("ALTER TABLE {table} RENAME TO {}", ident(name))
        }
        other => return Err(refused(format!("Not a native change: {other:?}"))),
    })
}
