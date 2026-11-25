//! Schema edit plans (task 9.5, D-042). MySQL commits every DDL statement, so changes are
//! gathered into as few `ALTER TABLE` statements as possible (one is atomic on MySQL 8 and on
//! MariaDB's copy algorithm). A new statement starts where one `ALTER` might not see the
//! effect of an earlier clause: a column renamed or a name dropped earlier in the statement, or
//! a table rename. Columns are altered from their `SHOW CREATE TABLE` text.

mod column;

use std::collections::{HashMap, HashSet};

use dbc_core::dialect::Dialect;
use dbc_core::schema::TableInfo;
use dbc_core::schema_edit::render::{check_clause, foreign_key_clause, index_parts, unique_clause};
use dbc_core::schema_edit::{SchemaChange, SchemaEdit, SchemaPlan, table_states};
use dbc_core::sql::definition::{ColumnParts, column_name, split_column, split_create_table};
use dbc_core::{DbError, DbErrorCode};

use crate::MysqlDialect;

pub(crate) fn refused(message: impl Into<String>) -> DbError {
    DbError::new(DbErrorCode::Execution, message)
}

fn ident(name: &str) -> String {
    MysqlDialect.quote_identifier(name)
}

/// The statements that make `edit`, given the table's `SHOW CREATE TABLE` text.
pub fn plan_schema_edit(edit: &SchemaEdit, create_table: &str) -> Result<SchemaPlan, DbError> {
    let states = table_states(&MysqlDialect, edit)?;
    let mut planner = Planner {
        columns: server_columns(create_table)?,
        ..Planner::default()
    };
    for (change, before) in edit.changes.iter().zip(&states) {
        planner.change(before, change)?;
    }
    planner.flush();
    if planner.statements.is_empty() {
        return Err(refused(
            "Nothing to apply: the changes leave the table as it is",
        ));
    }
    if planner.statements.len() > 1 {
        planner.warnings.push(format!(
            "MySQL cannot undo schema changes: these {} statements run one at a time, and if one \
             fails, the ones before it stay applied",
            planner.statements.len()
        ));
    }
    Ok(SchemaPlan {
        statements: planner.statements,
        transactional: false,
        destructive: edit.changes.iter().any(SchemaChange::is_destructive),
        warnings: planner.warnings,
        ..SchemaPlan::default()
    })
}

/// Each column's definition as the server wrote it, by unquoted name.
fn server_columns(create_table: &str) -> Result<HashMap<String, ColumnParts>, DbError> {
    let rules = MysqlDialect.splitter_rules();
    let parts = split_create_table(create_table, rules)
        .ok_or_else(|| refused("Could not read the table's CREATE TABLE statement"))?;
    Ok(parts
        .items
        .iter()
        .filter_map(|item| Some((column_name(item, rules)?, split_column(item, rules)?)))
        .collect())
}

#[derive(Default)]
struct Planner {
    columns: HashMap<String, ColumnParts>,
    statements: Vec<String>,
    warnings: Vec<String>,
    /// The `ALTER TABLE` being gathered: its table and clauses.
    table: String,
    clauses: Vec<String>,
    /// Columns renamed and names dropped in the current statement.
    renamed: HashSet<String>,
    dropped: HashSet<String>,
}

impl Planner {
    fn flush(&mut self) {
        if !self.clauses.is_empty() {
            self.statements.push(format!(
                "ALTER TABLE {} {}",
                self.table,
                self.clauses.join(", ")
            ));
        }
        self.clauses.clear();
        self.renamed.clear();
        self.dropped.clear();
    }

    /// Adds a clause to the current statement, starting a new one first when the clause names
    /// a column renamed or an object dropped earlier in it.
    fn push(&mut self, table: &str, clause: String, columns: &[&str], adds: Option<&str>) {
        let conflict = columns.iter().any(|c| self.renamed.contains(*c))
            || adds.is_some_and(|name| self.dropped.contains(name));
        if conflict || self.table != table {
            self.flush();
        }
        self.table = table.to_owned();
        self.clauses.push(clause);
    }

    fn change(&mut self, before: &TableInfo, change: &SchemaChange) -> Result<(), DbError> {
        let table = MysqlDialect.qualified_name(&before.table);
        match change {
            SchemaChange::AddColumn { column } => {
                let definition = column::new_definition(column)?;
                self.push(&table, format!("ADD COLUMN {definition}"), &[], None);
                let rules = MysqlDialect.splitter_rules();
                if let Some(parts) = split_column(&definition, rules) {
                    self.columns.insert(column.name.clone(), parts);
                }
            }
            SchemaChange::AlterColumn {
                name,
                column,
                using,
            } => {
                if using.is_some() {
                    self.warnings.push(format!(
                        "MySQL has no USING: the values of {name} are converted by the server's \
                         own casting rules"
                    ));
                }
                self.alter_column(before, &table, name, column)?;
            }
            SchemaChange::DropColumn { name } => {
                self.push(
                    &table,
                    format!("DROP COLUMN {}", ident(name)),
                    &[name],
                    None,
                );
                self.columns.remove(name);
            }
            SchemaChange::AddPrimaryKey { key } => {
                let clause = format!(
                    "ADD PRIMARY KEY ({})",
                    dbc_core::schema_edit::render::column_list(&MysqlDialect, &key.columns)
                );
                self.push(&table, clause, &columns(&key.columns), Some("PRIMARY"));
            }
            SchemaChange::DropPrimaryKey => self.drop(&table, "DROP PRIMARY KEY".into(), "PRIMARY"),
            SchemaChange::AddUnique { key } => {
                let clause = format!("ADD {}", unique_clause(&MysqlDialect, key));
                self.push(&table, clause, &columns(&key.columns), key.name.as_deref());
            }
            SchemaChange::DropUnique { key } => {
                let name = named(key.name.as_deref(), "unique constraint")?;
                self.drop(&table, format!("DROP INDEX {}", ident(name)), name);
            }
            SchemaChange::AddForeignKey { foreign_key } => {
                let clause = format!("ADD {}", foreign_key_clause(&MysqlDialect, foreign_key));
                let name = foreign_key.name.as_deref();
                self.push(&table, clause, &columns(&foreign_key.columns), name);
            }
            SchemaChange::DropForeignKey { foreign_key } => {
                let name = named(foreign_key.name.as_deref(), "foreign key")?;
                self.drop(&table, format!("DROP FOREIGN KEY {}", ident(name)), name);
            }
            SchemaChange::AddCheck { check } => {
                let clause = format!("ADD {}", check_clause(&MysqlDialect, check));
                self.push(&table, clause, &[], check.name.as_deref());
            }
            SchemaChange::DropCheck { check } => {
                let name = named(check.name.as_deref(), "check constraint")?;
                self.drop(&table, format!("DROP CONSTRAINT {}", ident(name)), name);
            }
            SchemaChange::AddIndex { index } => {
                if index.predicate.is_some() {
                    return Err(refused("MySQL has no partial indexes (WHERE)"));
                }
                let mut clause = format!(
                    "ADD {}INDEX {} ({})",
                    if index.unique { "UNIQUE " } else { "" },
                    ident(&index.name),
                    index_parts(&MysqlDialect, before, &index.columns)
                );
                if let Some(method) = &index.method {
                    clause.push_str(&format!(" USING {}", method.to_ascii_uppercase()));
                }
                self.push(&table, clause, &columns(&index.columns), Some(&index.name));
            }
            SchemaChange::DropIndex { index } => {
                self.drop(
                    &table,
                    format!("DROP INDEX {}", ident(&index.name)),
                    &index.name,
                );
            }
            SchemaChange::RenameTable { name } => {
                self.push(&table, format!("RENAME TO {}", ident(name)), &[], None);
                self.flush();
            }
            SchemaChange::SetComment { comment } => {
                let literal = column::comment_literal(comment.as_deref().unwrap_or(""))?;
                self.push(&table, format!("COMMENT = {literal}"), &[], None);
            }
        }
        Ok(())
    }

    fn drop(&mut self, table: &str, clause: String, name: &str) {
        self.push(table, clause, &[], None);
        self.dropped.insert(name.to_owned());
    }

    fn alter_column(
        &mut self,
        before: &TableInfo,
        table: &str,
        name: &str,
        new: &dbc_core::schema_edit::ColumnDefinition,
    ) -> Result<(), DbError> {
        let old = before
            .column_named(name)
            .ok_or_else(|| refused(format!("{} has no column {name}", before.table.name)))?;
        let server = self.columns.get(name).ok_or_else(|| {
            refused(format!(
                "Could not find column {name} in the table's CREATE TABLE statement"
            ))
        })?;
        let parts = column::changed_definition(server, &old, new)?;
        if new.type_name != old.type_name {
            self.warnings.push(format!(
                "Changing the type of {name} rebuilds the table and may lock it while it runs"
            ));
        }
        let unchanged = new.name == name
            && parts.type_text == server.type_text
            && parts.clauses == server.clauses;
        if unchanged {
            return Ok(());
        }
        let clause = format!("CHANGE COLUMN {} {}", ident(name), parts.to_sql());
        self.push(table, clause, &[name], None);
        self.columns.remove(name);
        self.columns.insert(new.name.clone(), parts);
        if new.name != name {
            self.renamed.insert(new.name.clone());
        }
        Ok(())
    }
}

fn columns(names: &[String]) -> Vec<&str> {
    names.iter().map(String::as_str).collect()
}

fn named<'a>(name: Option<&'a str>, what: &str) -> Result<&'a str, DbError> {
    name.filter(|n| !n.is_empty())
        .ok_or_else(|| refused(format!("The {what} has no name, so it cannot be dropped")))
}
