//! Measured plans read from captured `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` output (task 10.3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dbc_driver_postgres::{plan_analyzed_from_json, plan_from_json};

#[test]
fn a_measured_plan_has_actual_rows_time_and_scans() {
    let plan = plan_from_json(include_str!("fixtures/explain_analyze.json")).unwrap();

    assert_eq!(plan.operation, "Sort");
    assert_eq!(plan.actual_rows, Some(207.0));
    assert_eq!(plan.loops, Some(1.0));
    assert_eq!(plan.actual_time_ms, Some(0.933));
    assert_eq!(plan.estimated_rows, Some(111.0));
    assert!(!plan.spilled);
    let join = &plan.children[0].children[0].children[0];
    assert_eq!(join.operation, "Hash Join");
    let scan = &join.children[0];
    assert!(scan.full_scan);
    assert_eq!(scan.table.as_deref(), Some("o"));
    assert_eq!(scan.filter.as_deref(), Some("(total = 5)"));
    assert_eq!(
        scan.rows_read,
        Some(20000.0),
        "returned plus removed by the filter"
    );
    assert!(
        scan.details
            .iter()
            .any(|d| d.starts_with("Buffers: shared hit="))
    );
}

#[test]
fn sorts_and_hashes_on_disk_are_spills() {
    let text = r#"[{"Plan": {"Node Type": "Sort", "Sort Space Type": "Disk", "Actual Total Time": 5.0, "Actual Loops": 2, "Actual Rows": 10,
        "Plans": [{"Node Type": "Hash", "Hash Batches": 4, "Plans": []},
                  {"Node Type": "Hash", "Hash Batches": 1}]}}]"#;
    let plan = plan_from_json(text).unwrap();
    assert!(plan.spilled);
    assert_eq!(plan.actual_time_ms, Some(10.0), "per loop times loops");
    assert!(plan.children[0].spilled);
    assert!(!plan.children[1].spilled);
}

#[test]
fn an_estimated_plan_has_no_measurements() {
    let text = r#"[{"Plan": {"Node Type": "Seq Scan", "Relation Name": "t", "Plan Rows": 5, "Total Cost": 1.0}}]"#;
    let plan = plan_from_json(text).unwrap();
    assert!(plan.full_scan);
    assert_eq!(plan.actual_rows, None);
    assert_eq!(plan.actual_time_ms, None);
    assert_eq!(plan.rows_read, None);
}

#[test]
fn the_execution_time_is_the_total() {
    let analyzed = plan_analyzed_from_json(include_str!("fixtures/explain_analyze.json")).unwrap();
    assert_eq!(analyzed.total_ms, Some(0.987));
    assert_eq!(analyzed.plan.operation, "Sort");
}
