//! Explain and explain analyze (D-029, D-043, tasks 10.6, 10.7): a plan with its hints, and a
//! suggested index only where the table exists and nothing already serves the column.

use std::collections::HashMap;

use dbc_core::DbError;
use dbc_core::plan::{self, AnalyzedPlan, PlanHint};
use dbc_core::schema::{Index, TableInfo, TableRef};
use dbc_core::sql::StatementClass;

use crate::error::AppError;
use crate::ipc::{
    PlanReport, QueryExplainAnalyzeArgs, QueryExplainArgs, ReportedHint, SuggestedIndex,
};
use crate::session::Session;
use crate::state::AppState;

fn one_statement(session: &Session, sql: &str, what: &str) -> Result<(), AppError> {
    if sql.trim().is_empty() {
        return Err(AppError::invalid(format!(
            "There is no statement to {what}"
        )));
    }
    if session.dialect.split(sql).len() > 1 {
        return Err(AppError::invalid(format!("{what} one statement at a time")));
    }
    Ok(())
}

/// The plan, without running the statement.
pub async fn query_explain(
    state: &AppState,
    args: QueryExplainArgs,
) -> Result<PlanReport, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    if !session.capabilities.explain {
        return Err(AppError::from(DbError::unsupported("explain")));
    }
    one_statement(&session, &args.sql, "Explain")?;
    let statement = args.sql.clone();
    let planned = session
        .on_tab(&args.tab_id, |connection| async move {
            connection.explain(&statement).await
        })
        .await;
    let plan = finish(state, &session, planned, &args.sql).await?;
    let analyzed = AnalyzedPlan {
        plan,
        total_ms: None,
    };
    Ok(report(&session, analyzed, false).await)
}

/// Runs the statement and returns its measured plan. A statement that writes runs in a
/// transaction that is rolled back, only where the driver can, and never in a manual
/// transaction, whose work the rollback would undo.
pub async fn query_explain_analyze(
    state: &AppState,
    args: QueryExplainAnalyzeArgs,
) -> Result<PlanReport, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    if !session.capabilities.explain_analyze {
        return Err(AppError::from(DbError::unsupported("explain analyze")));
    }
    one_statement(&session, &args.sql, "Explain analyze")?;
    let rollback = check_analyze(&session, &args)?;
    let statement = args.sql.clone();
    let measured = session
        .on_tab(&args.tab_id, |connection| async move {
            connection.explain_analyze(&statement, rollback).await
        })
        .await;
    let analyzed = finish(state, &session, measured, &args.sql).await?;
    Ok(report(&session, analyzed, true).await)
}

/// The safeguards of running the statement; returns whether it must be rolled back.
fn check_analyze(session: &Session, args: &QueryExplainAnalyzeArgs) -> Result<bool, AppError> {
    let classification = session.dialect.classify(&args.sql);
    let writes = match classification.class {
        StatementClass::Read => false,
        StatementClass::Write => true,
        _ => {
            return Err(AppError::invalid(
                "Explain analyze measures queries and data changes only",
            ));
        }
    };
    if writes && session.read_only {
        return Err(AppError::read_only());
    }
    if let Some(danger) = classification.danger
        && args.confirmed != Some(true)
    {
        return Err(AppError::confirmation_required(danger));
    }
    if writes && !session.capabilities.explain_analyze_writes {
        return Err(AppError::invalid(
            "This database cannot undo a measured change, so explain analyze is offered for queries only",
        ));
    }
    if writes && session.transaction(&args.tab_id).manual {
        return Err(AppError::invalid(
            "Switch the tab to auto-commit to explain analyze a change: undoing it would undo the open transaction too",
        ));
    }
    Ok(writes)
}

async fn finish<T>(
    state: &AppState,
    session: &Session,
    result: Result<T, DbError>,
    sql: &str,
) -> Result<T, AppError> {
    match result {
        Ok(value) => Ok(value),
        Err(_) if session.tunnel_lost() => {
            Err(super::queries::tunnel_lost(state, &session.id).await)
        }
        Err(error) => Err(AppError::from_db(error, sql)),
    }
}

/// Hints for the plan, with suggestions checked against the tables they name.
async fn report(session: &Session, analyzed: AnalyzedPlan, measured: bool) -> PlanReport {
    let hints = plan::hints(&analyzed.plan, session.dialect.splitter_rules());
    // Hints are a convenience: without a namespace or a table, suggestions are just dropped.
    let namespace = session
        .default_namespace()
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let mut tables = HashMap::new();
    for name in hints
        .iter()
        .filter_map(|h| h.suggestion.as_ref())
        .map(|s| &s.table)
    {
        if tables.contains_key(name) {
            continue;
        }
        let table = TableRef {
            namespace: namespace.clone(),
            name: name.clone(),
        };
        if let Ok(info) = session.schema_table(&table).await {
            tables.insert(name.clone(), info);
        }
    }
    PlanReport {
        hints: reported_hints(hints, &tables),
        plan: analyzed.plan,
        measured,
        total_ms: analyzed.total_ms,
    }
}

/// Keeps a suggestion only for a table that exists and lacks an index on the column.
pub(crate) fn reported_hints(
    hints: Vec<PlanHint>,
    tables: &HashMap<String, TableInfo>,
) -> Vec<ReportedHint> {
    hints
        .into_iter()
        .map(|hint| {
            let index = hint.suggestion.and_then(|s| {
                let info = tables.get(&s.table)?;
                plan::index_is_missing(info, &s.column).then(|| SuggestedIndex {
                    table: info.table.clone(),
                    index: Index {
                        name: format!("{}_{}_idx", info.table.name, s.column),
                        columns: vec![s.column],
                        unique: false,
                        method: None,
                        predicate: None,
                    },
                })
            });
            ReportedHint {
                path: hint.path,
                kind: hint.kind,
                message: hint.message,
                index,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use dbc_core::plan::{HintKind, IndexSuggestion};
    use dbc_core::result::TypeCategory;
    use dbc_core::schema::{Column, NamespaceRef, ObjectKind};

    use super::*;

    fn info(columns: &[&str], indexed: &[&str]) -> TableInfo {
        TableInfo {
            table: TableRef {
                namespace: NamespaceRef {
                    catalog: None,
                    schema: Some("public".into()),
                },
                name: "orders".into(),
            },
            kind: ObjectKind::Table,
            comment: None,
            columns: columns
                .iter()
                .map(|name| Column {
                    name: (*name).into(),
                    type_name: "integer".into(),
                    category: TypeCategory::Integer,
                    nullable: true,
                    default: None,
                    comment: None,
                    ordinal: 1,
                    generated: false,
                })
                .collect(),
            primary_key: None,
            indexes: indexed
                .iter()
                .map(|c| Index {
                    name: format!("i_{c}"),
                    columns: vec![(*c).into()],
                    unique: false,
                    method: None,
                    predicate: None,
                })
                .collect(),
            foreign_keys: vec![],
            unique_constraints: vec![],
            check_constraints: vec![],
            row_key: None,
            row_id: None,
        }
    }

    fn scan_hint(table: &str, column: &str) -> PlanHint {
        PlanHint {
            path: vec![0],
            kind: HintKind::FullScan,
            message: "Reads every row".into(),
            suggestion: Some(IndexSuggestion {
                table: table.into(),
                column: column.into(),
            }),
        }
    }

    #[test]
    fn a_suggestion_becomes_a_named_index_on_the_real_table() {
        let tables = HashMap::from([("orders".to_owned(), info(&["id", "total"], &[]))]);
        let reported = reported_hints(vec![scan_hint("orders", "total")], &tables);
        let index = reported[0].index.as_ref().unwrap();
        assert_eq!(index.table.namespace.schema.as_deref(), Some("public"));
        assert_eq!(index.index.name, "orders_total_idx");
        assert_eq!(index.index.columns, ["total"]);
    }

    #[test]
    fn suggestions_for_unknown_tables_or_indexed_columns_are_dropped() {
        let tables = HashMap::from([("orders".to_owned(), info(&["id", "total"], &["total"]))]);
        let reported = reported_hints(
            vec![
                scan_hint("orders", "total"),
                scan_hint("o", "total"),
                scan_hint("orders", "gone"),
            ],
            &tables,
        );
        assert!(reported.iter().all(|h| h.index.is_none()));
        assert_eq!(reported.len(), 3, "the hints stay");
    }
}
