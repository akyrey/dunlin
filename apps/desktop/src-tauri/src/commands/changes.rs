//! Staged edits (task 7.5): preview and apply a change set on a table view's connection.
//! Every rule is checked here, not only in the UI: read-only sessions, views and tables without
//! a row key cannot be edited, generated and identity columns are never written, and rows are
//! addressed by exactly the row key.

use dbc_core::changes::{self, ChangeSet, ColumnType, RowOperation};
use dbc_core::schema::{Column, ObjectKind, TableInfo};
use dbc_core::{DbErrorCode, Value};
use serde_json::json;

use crate::error::AppError;
use crate::ipc::{ApplyResult, ChangesArgs, KeyArg, OperationArg};
use crate::session::Session;
use crate::state::AppState;

/// `GENERATED ALWAYS AS IDENTITY` columns refuse explicit values; by-default ones accept them.
fn writable(column: &Column) -> bool {
    !column.generated && column.default.as_deref() != Some("GENERATED ALWAYS AS IDENTITY")
}

fn editable(session: &Session, info: &TableInfo) -> Result<Vec<String>, AppError> {
    if session.read_only {
        return Err(AppError::read_only());
    }
    if info.kind != ObjectKind::Table {
        return Err(AppError::invalid("Only tables can be edited, not views"));
    }
    info.row_key.clone().ok_or_else(|| {
        AppError::invalid("The table has no primary key or unique not-null key, so it is read-only")
    })
}

fn values(
    pairs: Vec<KeyArg>,
    info: &TableInfo,
    writing: bool,
) -> Result<Vec<(String, Value)>, AppError> {
    pairs
        .into_iter()
        .map(|KeyArg { column, value }| {
            let described = info
                .columns
                .iter()
                .find(|c| c.name == column)
                .ok_or_else(|| {
                    AppError::invalid(format!("{} has no column {column}", info.table.name))
                })?;
            if writing && !writable(described) {
                return Err(AppError::invalid(format!(
                    "{column} is generated and cannot be written"
                )));
            }
            let value = Value::try_from(value).map_err(|e| {
                AppError::invalid(format!("The value for {column} cannot be written: {e}"))
            })?;
            Ok((column, value))
        })
        .collect()
}

fn key(
    pairs: Vec<KeyArg>,
    info: &TableInfo,
    row_key: &[String],
) -> Result<Vec<(String, Value)>, AppError> {
    let key = values(pairs, info, false)?;
    let mut given: Vec<&str> = key.iter().map(|(c, _)| c.as_str()).collect();
    let mut wanted: Vec<&str> = row_key.iter().map(String::as_str).collect();
    given.sort_unstable();
    wanted.sort_unstable();
    if given != wanted {
        return Err(AppError::invalid(format!(
            "A row is addressed by its key: {}",
            row_key.join(", ")
        )));
    }
    Ok(key)
}

async fn change_set(
    session: &Session,
    state: &AppState,
    args: &ChangesArgs,
) -> Result<ChangeSet, AppError> {
    let info = session.schema_table(&args.table).await;
    let info = super::table_data::finish(state, session, info).await?;
    let row_key = editable(session, &info)?;
    let operations = args
        .operations
        .iter()
        .cloned()
        .map(|operation| {
            Ok(match operation {
                OperationArg::Insert { values: v } => RowOperation::Insert {
                    values: values(v, &info, true)?,
                },
                OperationArg::Update { key: k, set } => RowOperation::Update {
                    key: key(k, &info, &row_key)?,
                    set: values(set, &info, true)?,
                },
                OperationArg::Delete { key: k } => RowOperation::Delete {
                    key: key(k, &info, &row_key)?,
                },
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(ChangeSet {
        table: args.table.clone(),
        columns: info
            .columns
            .iter()
            .map(|c| ColumnType {
                name: c.name.clone(),
                type_name: c.type_name.clone(),
            })
            .collect(),
        operations,
    })
}

/// The statements Apply would run, with literals, in order.
pub async fn changes_preview(state: &AppState, args: ChangesArgs) -> Result<Vec<String>, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let set = change_set(&session, state, &args).await?;
    changes::preview(session.dialect.as_ref(), &set).map_err(AppError::from)
}

/// Applies the change set in one transaction on the view's connection. A failure carries the
/// index of the operation (in `operations`) that caused it in `detail.operation`.
pub async fn changes_apply(state: &AppState, args: ChangesArgs) -> Result<ApplyResult, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let set = change_set(&session, state, &args).await?;
    let dialect = std::sync::Arc::clone(&session.dialect);
    let outcome = session
        .on_tab(&args.view_id, |connection| async move {
            match changes::apply(connection.as_ref(), dialect.as_ref(), &set).await {
                Ok(report) => Ok(Ok(report)),
                // A lost connection is the session's business (reconnect, tunnel).
                Err(failure) if failure.error.code == DbErrorCode::ConnectionLost => {
                    Err(failure.error)
                }
                Err(failure) => Ok(Err(failure)),
            }
        })
        .await;
    match super::table_data::finish(state, &session, outcome).await? {
        Ok(report) => Ok(ApplyResult {
            statements: report.statements,
            rows_affected: report.rows_affected.to_string(),
        }),
        Err(failure) => {
            let error = AppError::from(failure.error);
            let mut detail = error.detail.clone().unwrap_or_else(|| json!({}));
            detail["operation"] = json!(failure.operation);
            Err(error.with_detail(detail))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbc_core::WireValue;
    use dbc_core::result::TypeCategory;
    use dbc_core::value::TextPayload;

    fn column(name: &str, generated: bool, default: Option<&str>) -> Column {
        Column {
            name: name.into(),
            type_name: "integer".into(),
            category: TypeCategory::Integer,
            nullable: true,
            default: default.map(str::to_owned),
            comment: None,
            ordinal: 1,
            generated,
        }
    }

    #[test]
    fn generated_and_identity_always_columns_are_not_writable() {
        assert!(writable(&column("a", false, None)));
        assert!(writable(&column(
            "b",
            false,
            Some("GENERATED BY DEFAULT AS IDENTITY")
        )));
        assert!(!writable(&column("c", true, Some("a + b"))));
        assert!(!writable(&column(
            "d",
            false,
            Some("GENERATED ALWAYS AS IDENTITY")
        )));
    }

    #[test]
    fn wire_values_must_be_whole() {
        let truncated = WireValue::Text(TextPayload {
            v: "cut".into(),
            truncated: true,
        });
        assert!(Value::try_from(truncated).is_err());
    }
}
