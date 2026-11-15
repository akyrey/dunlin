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
