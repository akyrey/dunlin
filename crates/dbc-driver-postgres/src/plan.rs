//! `EXPLAIN (FORMAT JSON)` output as a `PlanNode` tree.

use dbc_core::plan::PlanNode;
use dbc_core::{DbError, DbErrorCode};
use serde_json::Value as Json;

/// Keys shown as details, in this order, when present.
const DETAIL_KEYS: &[&str] = &[
    "Join Type",
    "Index Name",
    "Index Cond",
    "Recheck Cond",
    "Hash Cond",
    "Merge Cond",
    "Join Filter",
    "Filter",
    "Sort Key",
    "Group Key",
    "Strategy",
];

/// `[{"Plan": {...}}]`, the one value `EXPLAIN (FORMAT JSON)` returns.
pub fn from_json(text: &str) -> Result<PlanNode, DbError> {
    let json: Json = serde_json::from_str(text)
        .map_err(|e| DbError::new(DbErrorCode::Internal, format!("Unreadable plan: {e}")))?;
    let plan = json
        .get(0)
        .and_then(|first| first.get("Plan"))
        .ok_or_else(|| DbError::new(DbErrorCode::Internal, "The plan has no \"Plan\" node"))?;
    Ok(node(plan, 0))
}

/// The measured plan and the server's `Execution Time` from `EXPLAIN (ANALYZE, FORMAT JSON)`.
pub fn analyzed_from_json(text: &str) -> Result<dbc_core::plan::AnalyzedPlan, DbError> {
    let total_ms = serde_json::from_str::<Json>(text)
        .ok()
        .and_then(|json| json.get(0)?.get("Execution Time")?.as_f64());
    Ok(dbc_core::plan::AnalyzedPlan {
        plan: from_json(text)?,
        total_ms,
    })
}

/// Deep enough for any plan PostgreSQL produces.
const MAX_DEPTH: usize = 128;

fn node(json: &Json, depth: usize) -> PlanNode {
    let text = |key: &str| json.get(key).and_then(Json::as_str).map(str::to_owned);
    let details: Vec<String> = DETAIL_KEYS
        .iter()
        .filter_map(|key| {
            let value = json.get(*key)?;
            let shown = match value {
                Json::String(s) => s.clone(),
                Json::Array(items) => items
                    .iter()
                    .map(|i| i.as_str().map_or_else(|| i.to_string(), str::to_owned))
                    .collect::<Vec<_>>()
                    .join(", "),
                other => other.to_string(),
            };
            Some(format!("{key}: {shown}"))
        })
        .collect();
    let relation = text("Relation Name").map(|name| match text("Alias") {
        Some(alias) if alias != name => format!("{name} {alias}"),
        _ => name,
    });
    let children = match (
        json.get("Plans").and_then(Json::as_array),
        depth < MAX_DEPTH,
    ) {
        (Some(plans), true) => plans.iter().map(|p| node(p, depth + 1)).collect(),
        _ => Vec::new(),
    };
    let number = |key: &str| json.get(key).and_then(Json::as_f64);
    let operation = text("Node Type").unwrap_or_else(|| "?".to_owned());
    let full_scan = operation == "Seq Scan";
    let loops = number("Actual Loops");
    let actual_rows = number("Actual Rows");
    let removed = number("Rows Removed by Filter").unwrap_or(0.0);
    PlanNode {
        table: text("Relation Name"),
        filter: text("Filter"),
        rows_read: if full_scan {
            actual_rows.map(|rows| rows + removed)
        } else {
            None
        },
        actual_time_ms: number("Actual Total Time").map(|ms| ms * loops.unwrap_or(1.0)),
        spilled: text("Sort Space Type").as_deref() == Some("Disk")
            || number("Hash Batches").is_some_and(|batches| batches > 1.0),
        actual_rows,
        loops,
        full_scan,
        operation,
        relation,
        details: [details, buffers(json)].concat(),
        estimated_rows: number("Plan Rows"),
        cost: number("Total Cost"),
        children,
    }
}

/// `Buffers: shared hit=118 read=2`, when the plan was measured with `BUFFERS`.
fn buffers(json: &Json) -> Vec<String> {
    let count = |key: &str| json.get(key).and_then(Json::as_u64).unwrap_or(0);
    if json.get("Shared Hit Blocks").is_none() {
        return Vec::new();
    }
    let mut line = format!("Buffers: shared hit={}", count("Shared Hit Blocks"));
    for (key, label) in [
        ("Shared Read Blocks", "read"),
        ("Temp Read Blocks", "temp read"),
        ("Temp Written Blocks", "temp written"),
    ] {
        let blocks = count(key);
        if blocks > 0 {
            line.push_str(&format!(" {label}={blocks}"));
        }
    }
    vec![line]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn reads_nodes_relations_conditions_and_estimates() {
        let text = r#"[{"Plan": {"Node Type": "Hash Join", "Join Type": "Inner",
            "Total Cost": 45.5, "Plan Rows": 120, "Hash Cond": "(o.customer_id = c.id)",
            "Plans": [
              {"Node Type": "Seq Scan", "Relation Name": "orders", "Alias": "o",
               "Total Cost": 20.0, "Plan Rows": 1000, "Filter": "(paid)"},
              {"Node Type": "Hash", "Total Cost": 10.0, "Plan Rows": 50, "Plans": [
                {"Node Type": "Index Scan", "Relation Name": "customers", "Alias": "customers",
                 "Index Name": "customers_pkey", "Total Cost": 8.0, "Plan Rows": 50}]}]}}]"#;

        let plan = from_json(text).unwrap();

        assert_eq!(plan.operation, "Hash Join");
        assert_eq!(plan.cost, Some(45.5));
        assert_eq!(
            plan.details,
            ["Join Type: Inner", "Hash Cond: (o.customer_id = c.id)"]
        );
        assert_eq!(plan.children[0].relation.as_deref(), Some("orders o"));
        assert_eq!(plan.children[0].estimated_rows, Some(1000.0));
        let index = &plan.children[1].children[0];
        assert_eq!(index.relation.as_deref(), Some("customers"));
        assert_eq!(index.details, ["Index Name: customers_pkey"]);
    }

    #[test]
    fn text_that_is_not_a_plan_is_an_error() {
        assert!(from_json("[]").is_err());
        assert!(from_json("not json").is_err());
    }
}
