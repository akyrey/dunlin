#![allow(clippy::unwrap_used)]

use super::*;
use crate::sql::SplitterRules;

fn node(operation: &str, children: Vec<PlanNode>) -> PlanNode {
    PlanNode {
        children,
        ..PlanNode::named(operation)
    }
}

fn timed(operation: &str, ms: f64, children: Vec<PlanNode>) -> PlanNode {
    PlanNode {
        actual_time_ms: Some(ms),
        ..node(operation, children)
    }
}

fn kinds(plan: &PlanNode) -> Vec<(Vec<usize>, HintKind)> {
    hints(plan, SplitterRules::default())
        .into_iter()
        .map(|h| (h.path, h.kind))
        .collect()
}

fn scan(table: &str, rows: f64, filter: Option<&str>) -> PlanNode {
    PlanNode {
        full_scan: true,
        table: Some(table.into()),
        relation: Some(table.into()),
        estimated_rows: Some(rows),
        filter: filter.map(str::to_owned),
        ..PlanNode::named("Seq Scan")
    }
}

#[test]
fn the_hottest_step_is_the_one_with_most_time_of_its_own() {
    // Join: 10 ms in all, children 7 and 2: its own time is 1. The 7 ms child is hottest.
    let plan = timed(
        "Hash Join",
        10.0,
        vec![
            timed("Seq Scan", 7.0, vec![]),
            timed("Hash", 2.0, vec![timed("Seq Scan", 1.5, vec![])]),
        ],
    );
    let found = hints(&plan, SplitterRules::default());
    let hottest: Vec<_> = found
        .iter()
        .filter(|h| h.kind == HintKind::Hottest)
        .collect();
    assert_eq!(hottest.len(), 1);
    assert_eq!(hottest[0].path, [0]);
    assert!(hottest[0].message.contains("70%"), "{}", hottest[0].message);
}

#[test]
fn without_times_the_hottest_step_is_by_cost() {
    let plan = PlanNode {
        cost: Some(100.0),
        children: vec![
            PlanNode {
                cost: Some(20.0),
                ..PlanNode::named("a")
            },
            PlanNode {
                cost: Some(30.0),
                ..PlanNode::named("b")
            },
        ],
        ..PlanNode::named("root")
    };
    assert_eq!(kinds(&plan), [(vec![], HintKind::Hottest)], "50 of its own");
}

#[test]
fn a_single_step_has_no_hottest_hint() {
    assert!(kinds(&timed("Result", 1.0, vec![])).is_empty());
}

#[test]
fn estimates_off_by_ten_times_either_way_are_misestimates() {
    let step = |estimated: f64, actual: f64| PlanNode {
        estimated_rows: Some(estimated),
        actual_rows: Some(actual),
        ..PlanNode::named("Index Scan")
    };
    let plan = node(
        "root",
        vec![
            step(100.0, 1000.0),
            step(5000.0, 499.0),
            step(100.0, 999.0),
            step(2.0, 40.0),
        ],
    );
    let found: Vec<_> = kinds(&plan)
        .into_iter()
        .filter(|(_, k)| *k == HintKind::Misestimate)
        .map(|(path, _)| path)
        .collect();
    assert_eq!(
        found,
        [vec![0], vec![1]],
        "under ten times and tiny counts are not flagged"
    );
}

#[test]
fn full_scans_of_large_tables_suggest_an_index_for_a_one_column_filter() {
    let plan = node(
        "root",
        vec![
            scan("orders", 20_000.0, Some("(total = 5)")),
            scan("tiny", 9_999.0, Some("(x = 1)")),
        ],
    );
    let found = hints(&plan, SplitterRules::default());
    let scans: Vec<_> = found
        .iter()
        .filter(|h| h.kind == HintKind::FullScan)
        .collect();
    assert_eq!(scans.len(), 1);
    assert_eq!(scans[0].path, [0]);
    assert_eq!(
        scans[0].suggestion,
        Some(IndexSuggestion {
            table: "orders".into(),
            column: "total".into()
        })
    );
}

#[test]
fn a_filter_step_above_the_scan_counts_as_its_filter() {
    // MySQL: `Filter: ...` with the table scan as its only child.
    let plan = PlanNode {
        filter: Some("(o.total = 5)".into()),
        children: vec![PlanNode {
            rows_read: Some(20_000.0),
            ..scan("o", 20_000.0, None)
        }],
        ..PlanNode::named("Filter: (o.total = 5)")
    };
    let found = hints(&plan, SplitterRules::default());
    let scan_hint = found.iter().find(|h| h.kind == HintKind::FullScan).unwrap();
    assert_eq!(scan_hint.suggestion.as_ref().unwrap().column, "total");
}

#[test]
fn a_full_scan_without_a_simple_filter_has_no_suggestion() {
    let plan = node(
        "root",
        vec![
            scan("orders", 50_000.0, Some("(a = 1) OR (b = 2)")),
            scan("o2", 50_000.0, None),
        ],
    );
    let found = hints(&plan, SplitterRules::default());
    assert!(
        found
            .iter()
            .filter(|h| h.kind == HintKind::FullScan)
            .all(|h| h.suggestion.is_none())
    );
    assert_eq!(
        found
            .iter()
            .filter(|h| h.kind == HintKind::FullScan)
            .count(),
        2
    );
}

#[test]
fn spills_and_many_loops_are_flagged() {
    let plan = node(
        "root",
        vec![
            PlanNode {
                spilled: true,
                ..PlanNode::named("Sort")
            },
            PlanNode {
                loops: Some(1_000.0),
                ..PlanNode::named("Index Scan")
            },
            PlanNode {
                loops: Some(999.0),
                ..PlanNode::named("Index Scan")
            },
        ],
    );
    let found = kinds(&plan);
    assert!(found.contains(&(vec![0], HintKind::Spill)));
    assert!(found.contains(&(vec![1], HintKind::ManyLoops)));
    assert!(!found.iter().any(|(p, _)| p == &vec![2]));
}

#[test]
fn a_suggested_index_is_new_only_when_nothing_starts_with_its_column() {
    use crate::result::TypeCategory;
    use crate::schema::{Column, Index, Key, NamespaceRef, ObjectKind, TableInfo, TableRef};
    let column = |name: &str| Column {
        name: name.into(),
        type_name: "int".into(),
        category: TypeCategory::Integer,
        nullable: true,
        default: None,
        comment: None,
        ordinal: 1,
        generated: false,
    };
    let info = TableInfo {
        table: TableRef {
            namespace: NamespaceRef::default(),
            name: "o".into(),
        },
        kind: ObjectKind::Table,
        comment: None,
        columns: vec![column("id"), column("a"), column("b"), column("c")],
        primary_key: Some(Key {
            name: None,
            columns: vec!["id".into()],
        }),
        indexes: vec![Index {
            name: "i".into(),
            columns: vec!["a".into(), "b".into()],
            unique: false,
            method: None,
            predicate: None,
        }],
        foreign_keys: vec![],
        unique_constraints: vec![],
        check_constraints: vec![],
        row_key: None,
        row_id: None,
    };
    assert!(!index_is_missing(&info, "id"));
    assert!(!index_is_missing(&info, "a"));
    assert!(
        index_is_missing(&info, "b"),
        "b is second in an index, which does not serve it"
    );
    assert!(index_is_missing(&info, "c"));
    assert!(!index_is_missing(&info, "missing"));
}
