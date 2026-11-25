#![allow(clippy::unwrap_used)]

use super::*;
use crate::fake::{Scripted, block_on};
use crate::schema::{NamespaceRef, ObjectKind, TableRef};

fn plan(transactional: bool) -> SchemaPlan {
    SchemaPlan {
        before: vec!["PRAGMA foreign_keys = OFF".into()],
        statements: vec!["ALTER 1".into(), "ALTER 2".into(), "ALTER 3".into()],
        after: vec!["PRAGMA foreign_keys = ON".into()],
        transactional,
        destructive: false,
        warnings: vec![],
    }
}

fn failing(statements: Vec<&'static str>) -> Scripted {
    Scripted {
        failing: statements,
        ..Scripted::default()
    }
}

#[test]
fn a_transactional_plan_runs_in_one_transaction_between_before_and_after() {
    let connection = Scripted::default();
    let report = block_on(apply(&connection, &plan(true))).unwrap();
    assert_eq!(report.statements, 3);
    assert_eq!(
        connection.log(),
        [
            "PRAGMA foreign_keys = OFF",
            "BEGIN",
            "ALTER 1",
            "ALTER 2",
            "ALTER 3",
            "COMMIT",
            "PRAGMA foreign_keys = ON"
        ]
    );
}

#[test]
fn a_failing_transactional_statement_rolls_back_and_still_runs_after() {
    let connection = failing(vec!["ALTER 2"]);
    let failure = block_on(apply(&connection, &plan(true))).unwrap_err();
    assert_eq!(failure.statement, Some(1));
    assert_eq!(failure.applied, 0);
    assert_eq!(failure.error.message, "ALTER 2 failed");
    assert_eq!(
        connection.log(),
        [
            "PRAGMA foreign_keys = OFF",
            "BEGIN",
            "ALTER 1",
            "ALTER 2",
            "ROLLBACK",
            "PRAGMA foreign_keys = ON"
        ]
    );
}

#[test]
fn a_failing_commit_rolls_back() {
    let connection = failing(vec!["COMMIT"]);
    let failure = block_on(apply(&connection, &plan(true))).unwrap_err();
    assert_eq!(failure.statement, None);
    assert_eq!(failure.applied, 0);
    assert_eq!(
        connection.log()[5..],
        ["COMMIT", "ROLLBACK", "PRAGMA foreign_keys = ON"]
    );
}

#[test]
fn a_non_transactional_plan_stops_at_the_first_failure_and_counts_what_stayed() {
    let connection = failing(vec!["ALTER 3"]);
    let failure = block_on(apply(&connection, &plan(false))).unwrap_err();
    assert_eq!(failure.statement, Some(2));
    assert_eq!(failure.applied, 2);
    assert_eq!(
        connection.log(),
        [
            "PRAGMA foreign_keys = OFF",
            "ALTER 1",
            "ALTER 2",
            "ALTER 3",
            "PRAGMA foreign_keys = ON"
        ]
    );
}

#[test]
fn a_failing_before_statement_runs_nothing_else_but_after() {
    let connection = failing(vec!["PRAGMA foreign_keys = OFF"]);
    let failure = block_on(apply(&connection, &plan(true))).unwrap_err();
    assert_eq!(failure.statement, None);
    assert_eq!(failure.applied, 0);
    assert_eq!(
        connection.log(),
        ["PRAGMA foreign_keys = OFF", "PRAGMA foreign_keys = ON"]
    );
}

#[test]
fn a_failing_after_statement_is_reported_once_the_changes_are_in() {
    let connection = failing(vec!["PRAGMA foreign_keys = ON"]);
    let failure = block_on(apply(&connection, &plan(false))).unwrap_err();
    assert_eq!(failure.statement, None);
    assert_eq!(failure.applied, 3);
}

#[test]
fn the_first_error_wins_over_a_failing_after_statement() {
    let connection = failing(vec!["ALTER 1", "PRAGMA foreign_keys = ON"]);
    let failure = block_on(apply(&connection, &plan(true))).unwrap_err();
    assert_eq!(failure.error.message, "ALTER 1 failed");
}

#[test]
fn an_empty_plan_is_refused() {
    let connection = Scripted::default();
    let failure = block_on(apply(&connection, &SchemaPlan::default())).unwrap_err();
    assert!(failure.error.message.contains("Nothing to apply"));
    assert!(connection.log().is_empty());
}

fn info() -> TableInfo {
    TableInfo {
        table: TableRef {
            namespace: NamespaceRef::default(),
            name: "t".into(),
        },
        kind: ObjectKind::Table,
        comment: None,
        columns: vec![],
        primary_key: None,
        indexes: vec![],
        foreign_keys: vec![],
        unique_constraints: vec![],
        check_constraints: vec![],
        row_key: None,
        row_id: None,
    }
}

#[test]
fn the_table_must_still_match_the_description_the_edit_was_made_against() {
    let connection = Scripted {
        described: Some(info()),
        ..Scripted::default()
    };
    block_on(check_current(&connection, &info())).unwrap();
    let mut stale = info();
    stale.comment = Some("old".into());
    let error = block_on(check_current(&connection, &stale)).unwrap_err();
    assert_eq!(error.code, DbErrorCode::Execution);
    assert!(error.message.contains("changed since"), "{}", error.message);
}

#[test]
fn a_table_that_is_gone_is_not_found() {
    let error = block_on(check_current(&Scripted::default(), &info())).unwrap_err();
    assert_eq!(error.code, DbErrorCode::NotFound);
}
