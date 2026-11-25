//! Schema edits (task 9.9, D-042): preview and apply on the session. Every rule is checked here,
//! not only in the UI: read-only sessions and drivers without schema editing are refused, a
//! destructive plan needs confirmation, and the table must still be as the edit saw it.

use std::sync::Arc;

use dbc_core::DbError;
use dbc_core::driver::Connection;
use dbc_core::schema_edit::{self, SchemaApplyFailure, SchemaPlan};
use dbc_core::sql::Danger;
use serde_json::json;

use crate::error::AppError;
use crate::ipc::{SchemaEditArgs, SchemaEditPreview, SchemaEditResult};
use crate::session::Session;
use crate::state::AppState;

const DESTRUCTIVE: &str =
    "The changes drop columns, keys, indexes or constraints, or rebuild the table";

fn allowed(session: &Session) -> Result<(), AppError> {
    if session.read_only {
        return Err(AppError::read_only());
    }
    if !session.capabilities.schema_editing {
        return Err(AppError::from(DbError::unsupported("schema editing")));
    }
    Ok(())
}

/// The statements Apply would run, with warnings, without running anything.
pub async fn schema_edit_preview(
    state: &AppState,
    args: SchemaEditArgs,
) -> Result<SchemaEditPreview, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    allowed(&session)?;
    let plan = session.schema_edit_plan(&args.edit).await;
    let plan = super::schema::finish(state, &session, plan).await?;
    Ok(SchemaEditPreview {
        statement_offset: plan.before.len(),
        statements: plan.preview(),
        warnings: plan.warnings,
        destructive: plan.destructive,
        transactional: plan.transactional,
    })
}

/// Plans again on a connection of its own, checks the table is unchanged, and applies. A
/// failure carries `statement` (index into the plan's statements, or `null`) and `applied`
/// (statements that stayed applied) in its detail.
pub async fn schema_edit_apply(
    state: &AppState,
    args: SchemaEditArgs,
) -> Result<SchemaEditResult, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    allowed(&session)?;
    let connection = session.schema_edit_connection().await;
    let connection = super::schema::finish(state, &session, connection).await?;
    let outcome = plan_and_apply(&connection, &args).await;
    // Closing only releases this connection; the outcome is what the user needs.
    let _ = connection.close().await;
    // DDL may have run even when it failed part-way (MySQL).
    session.invalidate_schema();
    match outcome {
        Ok(statements) => Ok(SchemaEditResult { statements }),
        Err(Failure::Refused(error)) => Err(error),
        Err(Failure::Applying(failure)) => {
            let error = AppError::from(failure.error);
            let mut detail = error.detail.clone().unwrap_or_else(|| json!({}));
            detail["statement"] = json!(failure.statement);
            detail["applied"] = json!(failure.applied);
            Err(error.with_detail(detail))
        }
    }
}

enum Failure {
    Refused(AppError),
    Applying(SchemaApplyFailure),
}

impl From<DbError> for Failure {
    fn from(error: DbError) -> Self {
        Self::Refused(AppError::from(error))
    }
}

async fn plan_and_apply(
    connection: &Arc<dyn Connection>,
    args: &SchemaEditArgs,
) -> Result<usize, Failure> {
    schema_edit::check_current(connection.as_ref(), &args.edit.original).await?;
    let plan: SchemaPlan = connection.plan_schema_edit(&args.edit).await?;
    if plan.destructive && args.confirmed != Some(true) {
        return Err(Failure::Refused(AppError::confirmation_required(
            Danger::Command(DESTRUCTIVE),
        )));
    }
    schema_edit::apply(connection.as_ref(), &plan)
        .await
        .map(|report| report.statements)
        .map_err(Failure::Applying)
}
