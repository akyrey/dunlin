//! Staged edits (Phase 7, D-005): a `ChangeSet` becomes statements run in one transaction.
//!
//! Every statement is built once as a list of [`Part`]s; the executed form (placeholders and
//! bound values) and the preview (literals from `Dialect::render_literal`) are both rendered
//! from that one list, so the preview is exactly what runs, modulo parameters versus literals.

use crate::data::BoundSql;
use crate::dialect::Dialect;
use crate::driver::Connection;
use crate::error::{DbError, DbErrorCode};
use crate::result::{ExecuteOptions, StatementOutcome};
use crate::schema::TableRef;
use crate::value::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnType {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowOperation {
    /// Columns left out take their default.
    Insert {
        values: Vec<(String, Value)>,
    },
    /// `key` holds the row key's original values.
    Update {
        key: Vec<(String, Value)>,
        set: Vec<(String, Value)>,
    },
    Delete {
        key: Vec<(String, Value)>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChangeSet {
    pub table: TableRef,
    /// Every column an operation may name, with its type for placeholders and literals.
    pub columns: Vec<ColumnType>,
    pub operations: Vec<RowOperation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyReport {
    pub statements: usize,
    pub rows_affected: u64,
}

/// Why an apply failed, and which operation of the change set caused it.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{error}")]
pub struct ApplyFailure {
    /// Index into `ChangeSet::operations`; `None` for a failure outside one (begin, commit).
    pub operation: Option<usize>,
    pub error: DbError,
}

/// A piece of a statement: SQL text, or a value with the type of the column it goes into.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Sql(String),
    Value { value: Value, type_name: String },
}

/// One statement of a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedStatement {
    /// Index into `ChangeSet::operations`.
    pub operation: usize,
    pub parts: Vec<Part>,
    /// `UPDATE` and `DELETE` must affect exactly one row.
    pub one_row: bool,
}

impl PlannedStatement {
    /// The executed form: placeholders, with the values bound in order.
    pub fn bound<D: Dialect + ?Sized>(&self, dialect: &D) -> BoundSql {
        let mut sql = String::new();
        let mut params = Vec::new();
        for part in &self.parts {
            match part {
                Part::Sql(text) => sql.push_str(text),
                Part::Value { value, type_name } => {
                    params.push(value.clone());
                    sql.push_str(&dialect.placeholder(params.len(), type_name));
                }
            }
        }
        BoundSql { sql, params }
    }

    /// The preview: the same statement with readable literals. Never executed by the app.
    pub fn preview<D: Dialect + ?Sized>(&self, dialect: &D) -> String {
        self.parts
            .iter()
            .map(|part| match part {
                Part::Sql(text) => text.clone(),
                Part::Value { value, type_name } => dialect.render_literal(value, type_name),
            })
            .collect()
    }
}

fn invalid(message: impl Into<String>) -> DbError {
    DbError::new(DbErrorCode::Execution, message)
}

struct Builder<'a, D: Dialect + ?Sized> {
    dialect: &'a D,
    changes: &'a ChangeSet,
    parts: Vec<Part>,
}

impl<D: Dialect + ?Sized> Builder<'_, D> {
    fn sql(&mut self, text: &str) {
        match self.parts.last_mut() {
            Some(Part::Sql(last)) => last.push_str(text),
            _ => self.parts.push(Part::Sql(text.to_owned())),
        }
    }

    fn column(&mut self, name: &str) -> Result<String, DbError> {
        let column = self
            .changes
            .columns
            .iter()
            .find(|c| c.name == name)
            .ok_or_else(|| invalid(format!("{} has no column {name}", self.changes.table.name)))?;
        let quoted = self.dialect.quote_identifier(name);
        self.sql(&quoted);
        Ok(column.type_name.clone())
    }

    fn value(&mut self, value: &Value, type_name: String) {
        self.parts.push(Part::Value {
            value: value.clone(),
            type_name,
        });
    }

    /// ` WHERE a = ? AND b IS NULL`
    fn key(&mut self, key: &[(String, Value)]) -> Result<(), DbError> {
        if key.is_empty() {
            return Err(invalid("A row to change needs its key"));
        }
        self.sql(" WHERE ");
        for (i, (name, value)) in key.iter().enumerate() {
            if i > 0 {
                self.sql(" AND ");
            }
            let type_name = self.column(name)?;
            if *value == Value::Null {
                self.sql(" IS NULL");
            } else {
                self.sql(" = ");
                self.value(value, type_name);
            }
        }
        Ok(())
    }

    fn table(&mut self) {
        let name = self.dialect.qualified_name(&self.changes.table);
        self.sql(&name);
    }

    fn finish(self) -> Vec<Part> {
        self.parts
    }
}

fn statement<D: Dialect + ?Sized>(
    dialect: &D,
    changes: &ChangeSet,
    operation: &RowOperation,
) -> Result<Vec<Part>, DbError> {
    let mut b = Builder {
        dialect,
        changes,
        parts: Vec::new(),
    };
    match operation {
        RowOperation::Delete { key } => {
            b.sql("DELETE FROM ");
            b.table();
            b.key(key)?;
        }
        RowOperation::Update { key, set } => {
            if set.is_empty() {
                return Err(invalid("An update needs at least one column to set"));
            }
            b.sql("UPDATE ");
            b.table();
            b.sql(" SET ");
            for (i, (name, value)) in set.iter().enumerate() {
                if i > 0 {
                    b.sql(", ");
                }
                let type_name = b.column(name)?;
                b.sql(" = ");
                b.value(value, type_name);
            }
            b.key(key)?;
        }
        RowOperation::Insert { values } => {
            b.sql("INSERT INTO ");
            b.table();
            if values.is_empty() {
                b.sql(" ");
                b.sql(dialect.default_values());
            } else {
                b.sql(" (");
                let mut types = Vec::new();
                for (i, (name, _)) in values.iter().enumerate() {
                    if i > 0 {
                        b.sql(", ");
                    }
                    types.push(b.column(name)?);
                }
                b.sql(") VALUES (");
                for (i, ((_, value), type_name)) in values.iter().zip(types).enumerate() {
                    if i > 0 {
                        b.sql(", ");
                    }
                    b.value(value, type_name);
                }
                b.sql(")");
            }
        }
    }
    Ok(b.finish())
}

fn order(operation: &RowOperation) -> u8 {
    match operation {
        RowOperation::Delete { .. } => 0,
        RowOperation::Update { .. } => 1,
        RowOperation::Insert { .. } => 2,
    }
}

/// The statements of a change set: deletes, then updates, then inserts, each kind in the order
/// given.
pub fn plan<D: Dialect + ?Sized>(
    dialect: &D,
    changes: &ChangeSet,
) -> Result<Vec<PlannedStatement>, DbError> {
    let mut indexed: Vec<(usize, &RowOperation)> = changes.operations.iter().enumerate().collect();
    indexed.sort_by_key(|(i, operation)| (order(operation), *i));
    indexed
        .into_iter()
        .map(|(index, operation)| {
            Ok(PlannedStatement {
                operation: index,
                parts: statement(dialect, changes, operation)?,
                one_row: !matches!(operation, RowOperation::Insert { .. }),
            })
        })
        .collect()
}

/// The preview of every statement, in the order they run.
pub fn preview<D: Dialect + ?Sized>(
    dialect: &D,
    changes: &ChangeSet,
) -> Result<Vec<String>, DbError> {
    Ok(plan(dialect, changes)?
        .iter()
        .map(|s| s.preview(dialect))
        .collect())
}

/// Runs a change set in one transaction. Any error, or an `UPDATE`/`DELETE` that does not affect
/// exactly one row, rolls everything back.
pub async fn apply(
    connection: &dyn Connection,
    dialect: &dyn Dialect,
    changes: &ChangeSet,
) -> Result<ApplyReport, ApplyFailure> {
    let planned = plan(dialect, changes).map_err(|error| ApplyFailure {
        operation: None,
        error,
    })?;
    let options = ExecuteOptions::default();
    run(connection, "BEGIN", None).await?;
    let mut rows_affected = 0;
    for statement in &planned {
        let bound = statement.bound(dialect);
        let outcome = connection
            .query_params(&bound.sql, &bound.params, &options)
            .await;
        let checked = match outcome {
            Ok(StatementOutcome::Affected { rows }) if statement.one_row && rows != 1 => {
                Err(DbError::new(
                    DbErrorCode::RowCountMismatch,
                    format!(
                        "Expected to change exactly one row, but {rows} matched: the row was changed or deleted by someone else"
                    ),
                ))
            }
            Ok(StatementOutcome::Affected { rows }) => Ok(rows),
            Ok(StatementOutcome::Rows(_)) => Ok(0),
            Err(error) => Err(error),
        };
        match checked {
            Ok(rows) => rows_affected += rows,
            Err(error) => {
                rollback(connection).await;
                return Err(ApplyFailure {
                    operation: Some(statement.operation),
                    error,
                });
            }
        }
    }
    if let Err(failure) = run(connection, "COMMIT", None).await {
        rollback(connection).await;
        return Err(failure);
    }
    Ok(ApplyReport {
        statements: planned.len(),
        rows_affected,
    })
}

async fn run(
    connection: &dyn Connection,
    sql: &str,
    operation: Option<usize>,
) -> Result<(), ApplyFailure> {
    // Transaction control goes through the plain path: MySQL does not prepare every statement.
    connection
        .execute(sql, &ExecuteOptions::default())
        .await
        .map(|_| ())
        .map_err(|error| ApplyFailure { operation, error })
}

/// The error that caused the rollback is what the user needs; a failing rollback (the
/// connection dropped, which rolls back anyway) adds nothing to it.
async fn rollback(connection: &dyn Connection) {
    let _ = connection
        .execute("ROLLBACK", &ExecuteOptions::default())
        .await;
}

#[cfg(test)]
mod tests;
