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
pub(crate) fn from_json(text: &str) -> Result<PlanNode, DbError> {
    let json: Json = serde_json::from_str(text)
        .map_err(|e| DbError::new(DbErrorCode::Internal, format!("Unreadable plan: {e}")))?;
    let plan = json
        .get(0)
        .and_then(|first| first.get("Plan"))
        .ok_or_else(|| DbError::new(DbErrorCode::Internal, "The plan has no \"Plan\" node"))?;
    Ok(node(plan, 0))
}

/// Deep enough for any plan PostgreSQL produces.
const MAX_DEPTH: usize = 128;

fn node(json: &Json, depth: usize) -> PlanNode {
    let text = |key: &str| json.get(key).and_then(Json::as_str).map(str::to_owned);
    let details = DETAIL_KEYS
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
    PlanNode {
        operation: text("Node Type").unwrap_or_else(|| "?".to_owned()),
        relation,
        details,
        estimated_rows: json.get("Plan Rows").and_then(Json::as_f64),
        cost: json.get("Total Cost").and_then(Json::as_f64),
        children,
    }
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
