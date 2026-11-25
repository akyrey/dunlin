//! Runs a [`SchemaPlan`] on a connection.

use super::{SchemaPlan, refused};
use crate::driver::Connection;
use crate::error::{DbError, DbErrorCode};
use crate::result::ExecuteOptions;
use crate::schema::TableInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaApplyReport {
    pub statements: usize,
}

/// Why a plan failed, and how much of it stayed applied.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{error}")]
pub struct SchemaApplyFailure {
    /// Index into `SchemaPlan::statements`; `None` for a failure outside them (before, begin,
    /// commit, after).
    pub statement: Option<usize>,
    /// Statements that stayed applied: none after a rollback, the ones before the failing one
    /// in a plan without a transaction.
    pub applied: usize,
    pub error: DbError,
}

/// Refuses the edit if the table no longer has the description it was made against.
pub async fn check_current(
    connection: &dyn Connection,
    original: &TableInfo,
) -> Result<(), DbError> {
    let current = connection.describe_table(&original.table).await?;
    if &current != original {
        return Err(DbError::new(
            DbErrorCode::Execution,
            format!(
                "{} changed since you opened it. Reload it and make the changes again.",
                original.table.name
            ),
        ));
    }
    Ok(())
}

/// Runs `before`, the statements, then `after`, which runs whatever happened. A transactional
/// plan commits all its statements or none; otherwise they run one at a time and stop at the
/// first failure. The first error is the one reported.
pub async fn apply(
    connection: &dyn Connection,
    plan: &SchemaPlan,
) -> Result<SchemaApplyReport, SchemaApplyFailure> {
    if plan.statements.is_empty() {
        return Err(SchemaApplyFailure {
            statement: None,
            applied: 0,
            error: refused("Nothing to apply"),
        });
    }
    let outcome = run_before_and_statements(connection, plan).await;
    let applied = match &outcome {
        Ok(()) => plan.statements.len(),
        Err(failure) => failure.applied,
    };
    let after = run_all(connection, &plan.after).await;
    outcome?;
    after.map_err(|error| SchemaApplyFailure {
        statement: None,
        applied,
        error,
    })?;
    Ok(SchemaApplyReport {
        statements: plan.statements.len(),
    })
}

async fn run_before_and_statements(
    connection: &dyn Connection,
    plan: &SchemaPlan,
) -> Result<(), SchemaApplyFailure> {
    let outside = |error| SchemaApplyFailure {
        statement: None,
        applied: 0,
        error,
    };
    run_all(connection, &plan.before).await.map_err(outside)?;
    if !plan.transactional {
        return run_statements(connection, &plan.statements).await;
    }
    run(connection, "BEGIN").await.map_err(outside)?;
    if let Err(failure) = run_statements(connection, &plan.statements).await {
        rollback(connection).await;
        return Err(SchemaApplyFailure {
            applied: 0,
            ..failure
        });
    }
    if let Err(error) = run(connection, "COMMIT").await {
        rollback(connection).await;
        return Err(outside(error));
    }
    Ok(())
}

async fn run_statements(
    connection: &dyn Connection,
    statements: &[String],
) -> Result<(), SchemaApplyFailure> {
    for (index, statement) in statements.iter().enumerate() {
        run(connection, statement)
            .await
            .map_err(|error| SchemaApplyFailure {
                statement: Some(index),
                applied: index,
                error,
            })?;
    }
    Ok(())
}

/// Runs every statement, even after one fails, and returns the first error.
async fn run_all(connection: &dyn Connection, statements: &[String]) -> Result<(), DbError> {
    let mut first = Ok(());
    for statement in statements {
        let outcome = run(connection, statement).await;
        if first.is_ok() {
            first = outcome;
        }
    }
    first
}

async fn run(connection: &dyn Connection, sql: &str) -> Result<(), DbError> {
    connection
        .execute(sql, &ExecuteOptions::default())
        .await
        .map(|_| ())
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
