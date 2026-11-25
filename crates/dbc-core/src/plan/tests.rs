use super::*;

#[test]
fn an_indented_tree_keeps_its_nesting_estimates_and_details() {
    let text = "\
-> Nested loop inner join  (cost=2.5 rows=3)
    -> Filter: (p.balance > 0)  (cost=0.55 rows=1.67)
        -> Table scan on p  (cost=0.55 rows=5)
    -> Single-row index lookup on o using PRIMARY (id=p.id)  (cost=0.25 rows=1)
";

    let plan = from_indented(text);

    assert_eq!(plan.operation, "Nested loop inner join");
    assert_eq!(plan.cost, Some(2.5));
    assert_eq!(plan.estimated_rows, Some(3.0));
    let [filter, lookup] = plan.children.as_slice() else {
        panic!("{plan:?}")
    };
    assert_eq!(filter.estimated_rows, Some(1.67));
    assert_eq!(filter.children[0].relation.as_deref(), Some("p"));
    assert_eq!(lookup.relation.as_deref(), Some("o"));
    assert!(lookup.children.is_empty());
}

#[test]
fn several_top_level_steps_share_a_query_root() {
    let plan = from_indented("-> A\n-> B\n    -> C\n");

    assert_eq!(plan.operation, QUERY);
    let names: Vec<_> = plan.children.iter().map(|c| c.operation.as_str()).collect();
    assert_eq!(names, ["A", "B"]);
    assert_eq!(plan.children[1].children[0].operation, "C");
}

#[test]
fn parent_rows_become_a_tree() {
    let rows = [
        PlanRow {
            id: 2,
            parent: 0,
            text: "SCAN people".into(),
        },
        PlanRow {
            id: 5,
            parent: 0,
            text: "SEARCH orders USING INDEX o_p (person=?)".into(),
        },
        PlanRow {
            id: 8,
            parent: 5,
            text: "USE TEMP B-TREE FOR ORDER BY".into(),
        },
    ];

    let plan = from_parent_rows(&rows);

    assert_eq!(plan.operation, QUERY);
    assert_eq!(plan.children[0].relation.as_deref(), Some("people"));
    assert_eq!(plan.children[1].relation.as_deref(), Some("orders"));
    assert_eq!(
        plan.children[1].children[0].operation,
        "USE TEMP B-TREE FOR ORDER BY"
    );
}

#[test]
fn a_single_row_is_the_root_and_a_cycle_does_not_hang() {
    let single = from_parent_rows(&[PlanRow {
        id: 1,
        parent: 0,
        text: "SCAN t".into(),
    }]);
    let cycle = from_parent_rows(&[
        PlanRow {
            id: 1,
            parent: 2,
            text: "a".into(),
        },
        PlanRow {
            id: 2,
            parent: 1,
            text: "b".into(),
        },
    ]);

    assert_eq!(single.operation, "SCAN t");
    assert_eq!(cycle.operation, QUERY);
    assert!(cycle.children.is_empty());
}

proptest::proptest! {
    #[test]
    fn no_text_panics_the_indented_parser(text in "[ \\-> a-z(=.0-9)\n]{0,120}") {
        let _ = from_indented(&text);
    }
}

const MEASURED: &str = "\
-> Sort: `count(*)` DESC  (actual time=1.84..1.85 rows=207 loops=1)
    -> Table scan on <temporary>  (actual time=1.78..1.79 rows=207 loops=1)
        -> Nested loop inner join  (cost=2700 rows=2000) (actual time=0.101..1.72 rows=207 loops=1)
            -> Filter: ((o.total = 5) and (o.cid is not null))  (cost=2000 rows=2000) (actual time=0.0815..1.62 rows=207 loops=1)
                -> Table scan on o  (cost=2000 rows=20000) (actual time=0.0793..1.17 rows=20000 loops=1)
            -> Single-row index lookup on c using PRIMARY (id=o.cid)  (cost=0.25 rows=1) (actual time=313e-6..326e-6 rows=1 loops=207)
            -> Index lookup on x using i (id=o.cid)  (cost=0.25 rows=1) (never executed)
";

#[test]
fn a_measured_indented_tree_reads_actual_rows_loops_and_time() {
    let plan = from_indented(MEASURED);

    assert_eq!(plan.operation, "Sort: `count(*)` DESC");
    assert_eq!(plan.estimated_rows, None);
    assert_eq!(plan.actual_rows, Some(207.0));
    assert_eq!(plan.loops, Some(1.0));
    assert_eq!(plan.actual_time_ms, Some(1.85));
    let join = &plan.children[0].children[0];
    assert_eq!(join.estimated_rows, Some(2000.0));
    assert_eq!(join.cost, Some(2700.0));
    assert_eq!(join.actual_rows, Some(207.0));
    let filter = &join.children[0];
    assert_eq!(
        filter.filter.as_deref(),
        Some("((o.total = 5) and (o.cid is not null))")
    );
    assert!(!filter.full_scan);
    let scan = &filter.children[0];
    assert!(scan.full_scan);
    assert_eq!(scan.table.as_deref(), Some("o"));
    assert_eq!(scan.rows_read, Some(20000.0));
    let lookup = &join.children[1];
    assert!(!lookup.full_scan);
    assert_eq!(lookup.loops, Some(207.0));
    let per_loop = 326e-6_f64;
    assert!((lookup.actual_time_ms.unwrap() - per_loop * 207.0).abs() < 1e-9);
    let never = &join.children[2];
    assert_eq!(never.actual_rows, Some(0.0));
    assert_eq!(never.loops, Some(0.0));
    assert_eq!(never.actual_time_ms, Some(0.0));
}

#[test]
fn a_temporary_table_scan_is_not_a_full_scan() {
    let plan = from_indented(MEASURED);
    assert!(!plan.children[0].full_scan);
    assert_eq!(plan.children[0].table, None);
}

#[test]
fn sqlite_scans_of_a_table_are_full_scans() {
    let rows = [
        PlanRow {
            id: 2,
            parent: 0,
            text: "SCAN o".into(),
        },
        PlanRow {
            id: 3,
            parent: 0,
            text: "SCAN c USING COVERING INDEX c_name".into(),
        },
        PlanRow {
            id: 4,
            parent: 0,
            text: "SEARCH p USING INTEGER PRIMARY KEY (rowid=?)".into(),
        },
    ];
    let plan = from_parent_rows(&rows);
    assert!(plan.children[0].full_scan);
    assert_eq!(plan.children[0].table.as_deref(), Some("o"));
    assert!(!plan.children[1].full_scan);
    assert!(!plan.children[2].full_scan);
    assert_eq!(plan.children[2].table.as_deref(), Some("p"));
}
