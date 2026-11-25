//! Schema edit plans (task 9.4, D-042). Pure: one transaction, one statement per change (two
//! when a column is renamed and altered), in the order the changes were made.

use dbc_core::DbError;
use dbc_core::DbErrorCode;
use dbc_core::dialect::{Dialect, string_literal};
use dbc_core::schema::{Column, TableInfo, TableRef};
use dbc_core::schema_edit::render::{
    check_clause, default_clause, foreign_key_clause, index_parts, is_identity_or_generated,
    primary_key_clause, unique_clause,
};
use dbc_core::schema_edit::{ColumnDefinition, SchemaChange, SchemaEdit, SchemaPlan, table_states};

use crate::PostgresDialect;

const DIALECT: PostgresDialect = PostgresDialect;

fn refused(message: impl Into<String>) -> DbError {
    DbError::new(DbErrorCode::Execution, message)
}

fn ident(name: &str) -> String {
    DIALECT.quote_identifier(name)
}

fn comment_literal(comment: Option<&str>) -> String {
    comment.map_or_else(|| "NULL".to_owned(), string_literal)
}

/// The statements that make `edit`, validated by `table_states` first.
pub fn plan_schema_edit(edit: &SchemaEdit) -> Result<SchemaPlan, DbError> {
    let states = table_states(&DIALECT, edit)?;
    let mut plan = SchemaPlan {
        transactional: true,
        destructive: edit.changes.iter().any(SchemaChange::is_destructive),
        ..SchemaPlan::default()
    };
    for (change, before) in edit.changes.iter().zip(&states) {
        let step = change_statements(before, change)?;
        plan.statements.extend(step.statements);
        plan.warnings.extend(step.warnings);
    }
    if plan.statements.is_empty() {
        return Err(refused(
            "Nothing to apply: the changes leave the table as it is",
        ));
    }
    Ok(plan)
}

#[derive(Default)]
struct Step {
    statements: Vec<String>,
    warnings: Vec<String>,
}

impl Step {
    fn one(statement: String) -> Self {
        Self {
            statements: vec![statement],
            warnings: Vec::new(),
        }
    }
}

fn change_statements(before: &TableInfo, change: &SchemaChange) -> Result<Step, DbError> {
    let table = DIALECT.qualified_name(&before.table);
    let alter = format!("ALTER TABLE {table} ");
    let constraint = |name: Option<&str>, what: &str| -> Result<Step, DbError> {
        let name = name
            .filter(|n| !n.is_empty())
            .ok_or_else(|| refused(format!("The {what} has no name, so it cannot be dropped")))?;
        Ok(Step::one(format!("{alter}DROP CONSTRAINT {}", ident(name))))
    };
    Ok(match change {
        SchemaChange::AddColumn { column } => add_column(&table, column),
        SchemaChange::AlterColumn {
            name,
            column,
            using,
        } => alter_column(before, &table, name, column, using.as_deref())?,
        SchemaChange::DropColumn { name } => {
            Step::one(format!("{alter}DROP COLUMN {}", ident(name)))
        }
        SchemaChange::AddPrimaryKey { key } => {
            Step::one(format!("{alter}ADD {}", primary_key_clause(&DIALECT, key)))
        }
        SchemaChange::DropPrimaryKey => constraint(
            before.primary_key.as_ref().and_then(|k| k.name.as_deref()),
            "primary key",
        )?,
        SchemaChange::AddUnique { key } => {
            Step::one(format!("{alter}ADD {}", unique_clause(&DIALECT, key)))
        }
        SchemaChange::DropUnique { key } => constraint(key.name.as_deref(), "unique constraint")?,
        SchemaChange::AddForeignKey { foreign_key } => Step::one(format!(
            "{alter}ADD {}",
            foreign_key_clause(&DIALECT, foreign_key)
        )),
        SchemaChange::DropForeignKey { foreign_key } => {
            constraint(foreign_key.name.as_deref(), "foreign key")?
        }
        SchemaChange::AddCheck { check } => {
            Step::one(format!("{alter}ADD {}", check_clause(&DIALECT, check)))
        }
        SchemaChange::DropCheck { check } => constraint(check.name.as_deref(), "check constraint")?,
        SchemaChange::AddIndex { index } => {
            let mut sql = format!(
                "CREATE {}INDEX {} ON {table}",
                if index.unique { "UNIQUE " } else { "" },
                ident(&index.name)
            );
            if let Some(method) = &index.method {
                sql.push_str(&format!(" USING {method}"));
            }
            sql.push_str(&format!(
                " ({})",
                index_parts(&DIALECT, before, &index.columns)
            ));
            if let Some(predicate) = &index.predicate {
                sql.push_str(&format!(" WHERE {predicate}"));
            }
            Step::one(sql)
        }
        SchemaChange::DropIndex { index } => {
            let qualified = DIALECT.qualified_name(&TableRef {
                namespace: before.table.namespace.clone(),
                name: index.name.clone(),
            });
            Step::one(format!("DROP INDEX {qualified}"))
        }
        SchemaChange::RenameTable { name } => {
            Step::one(format!("{alter}RENAME TO {}", ident(name)))
        }
        SchemaChange::SetComment { comment } => Step::one(format!(
            "COMMENT ON TABLE {table} IS {}",
            comment_literal(comment.as_deref())
        )),
    })
}

fn column_definition(column: &ColumnDefinition) -> String {
    let mut sql = format!("{} {}", ident(&column.name), column.type_name);
    if !column.nullable {
        sql.push_str(" NOT NULL");
    }
    if let Some(default) = &column.default {
        sql.push(' ');
        sql.push_str(&default_clause(default));
    }
    sql
}

fn comment_on_column(table: &str, column: &str, comment: Option<&str>) -> String {
    format!(
        "COMMENT ON COLUMN {table}.{} IS {}",
        ident(column),
        comment_literal(comment)
    )
}

fn add_column(table: &str, column: &ColumnDefinition) -> Step {
    let mut statements = vec![format!(
        "ALTER TABLE {table} ADD COLUMN {}",
        column_definition(column)
    )];
    if let Some(comment) = &column.comment {
        statements.push(comment_on_column(table, &column.name, Some(comment)));
    }
    Step {
        statements,
        warnings: Vec::new(),
    }
}

fn alter_column(
    before: &TableInfo,
    table: &str,
    name: &str,
    new: &ColumnDefinition,
    using: Option<&str>,
) -> Result<Step, DbError> {
    let old = before
        .column_named(name)
        .ok_or_else(|| refused(format!("{} has no column {name}", before.table.name)))?;
    let mut step = Step::default();
    if new.name != name {
        step.statements.push(format!(
            "ALTER TABLE {table} RENAME COLUMN {} TO {}",
            ident(name),
            ident(&new.name)
        ));
    }
    let actions = column_actions(&old, new, using)?;
    if new.type_name != old.type_name {
        step.warnings.push(format!(
            "Changing the type of {} rewrites the table and locks it while it runs",
            new.name
        ));
    }
    if !actions.is_empty() {
        step.statements
            .push(format!("ALTER TABLE {table} {}", actions.join(", ")));
    }
    if new.comment != old.comment {
        step.statements
            .push(comment_on_column(table, &new.name, new.comment.as_deref()));
    }
    Ok(step)
}

/// The `ALTER COLUMN` actions of one alter. A default that changes with the type is dropped
/// first, as the old one may not cast to the new type.
fn column_actions(
    old: &Column,
    new: &ColumnDefinition,
    using: Option<&str>,
) -> Result<Vec<String>, DbError> {
    let column = format!("ALTER COLUMN {}", ident(&new.name));
    let type_changed = new.type_name != old.type_name;
    let default_changed = new.default != old.default;
    let identity = |d: &Option<String>| d.as_deref().is_some_and(is_identity_or_generated);
    if default_changed && (identity(&old.default) || identity(&new.default)) {
        return Err(refused(format!(
            "{} is an identity column: its default cannot be changed here",
            old.name
        )));
    }
    let mut actions = Vec::new();
    let mut dropped = false;
    if type_changed && default_changed && old.default.is_some() {
        actions.push(format!("{column} DROP DEFAULT"));
        dropped = true;
    }
    if type_changed {
        let using = using.map(|u| format!(" USING {u}")).unwrap_or_default();
        actions.push(format!("{column} TYPE {}{using}", new.type_name));
    }
    if default_changed {
        match &new.default {
            Some(default) => actions.push(format!("{column} SET DEFAULT {default}")),
            None if !dropped => actions.push(format!("{column} DROP DEFAULT")),
            None => {}
        }
    }
    if new.nullable != old.nullable {
        let verb = if new.nullable { "DROP" } else { "SET" };
        actions.push(format!("{column} {verb} NOT NULL"));
    }
    Ok(actions)
}
