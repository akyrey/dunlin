//! Measured plans read from captured MySQL and MariaDB output (task 10.4).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dbc_core::plan::from_indented;
use dbc_driver_mysql::plan_from_mariadb_json;

#[test]
fn mysql_explain_analyze_is_a_measured_tree() {
    let plan = from_indented(include_str!("fixtures/explain_analyze_mysql.txt"));
    assert_eq!(plan.actual_rows, Some(207.0));
    let join = &plan.children[0].children[0].children[0];
    let scan = &join.children[0].children[0];
    assert!(scan.full_scan);
    assert_eq!(scan.table.as_deref(), Some("o"));
    assert_eq!(scan.rows_read, Some(20000.0));
}

#[test]
fn mariadb_analyze_json_is_a_measured_tree() {
    let plan =
        plan_from_mariadb_json(include_str!("fixtures/explain_analyze_mariadb.json")).unwrap();

    assert_eq!(plan.operation, "Query block");
    let total = plan.actual_time_ms.unwrap();
    assert!((total - 1.289_913_439).abs() < 1e-9);
    let sort = &plan.children[0];
    assert_eq!(sort.operation, "Sort");
    assert!(sort.details.contains(&"Sort key: count(0) desc".to_owned()));
    let join = &sort.children[0].children[0];
    assert_eq!(join.operation, "Nested loop");
    let scan = &join.children[0];
    assert!(scan.full_scan);
    assert_eq!(scan.table.as_deref(), Some("o"));
    assert_eq!(scan.operation, "Full scan on o");
    assert_eq!(scan.estimated_rows, Some(19999.0));
    assert_eq!(scan.rows_read, Some(20000.0));
    assert_eq!(scan.actual_rows, Some(207.0), "rows read times r_filtered");
    assert_eq!(
        scan.filter.as_deref(),
        Some("o.total = 5 and o.cid is not null")
    );
    let own = 0.862_334_653 + 0.283_745_071;
    assert!((scan.actual_time_ms.unwrap() - own).abs() < 1e-9);
    let lookup = &join.children[1];
    assert!(!lookup.full_scan);
    assert_eq!(lookup.operation, "eq_ref on c using PRIMARY");
    assert_eq!(lookup.loops, Some(207.0));
    let sum = scan.actual_time_ms.unwrap() + lookup.actual_time_ms.unwrap();
    assert!(
        (join.actual_time_ms.unwrap() - sum).abs() < 1e-9,
        "a nested loop sums its tables"
    );
}

#[test]
fn unreadable_mariadb_output_is_an_error() {
    assert!(plan_from_mariadb_json("not json").is_err());
    assert!(plan_from_mariadb_json("{}").is_err());
}
