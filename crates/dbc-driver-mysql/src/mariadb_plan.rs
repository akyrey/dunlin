//! MariaDB's `ANALYZE FORMAT=JSON` as a measured plan tree (task 10.4, D-043).
//!
//! The JSON nests query blocks, sorts, temporary tables and nested loops, with one object per
//! table. Each table reports its own time (`r_table_time_ms` + `r_other_time_ms`, over all
//! loops); a step without a time of its own takes the sum of its children's.

use dbc_core::plan::PlanNode;
use dbc_core::{DbError, DbErrorCode};
use serde_json::{Map, Value as Json};

/// Deep enough for any plan MariaDB produces; guards against hostile input.
const MAX_DEPTH: usize = 64;

pub fn from_mariadb_json(text: &str) -> Result<PlanNode, DbError> {
    let json: Json = serde_json::from_str(text)
        .map_err(|e| DbError::new(DbErrorCode::Internal, format!("Unreadable plan: {e}")))?;
    let block = json
        .get("query_block")
        .ok_or_else(|| DbError::new(DbErrorCode::Internal, "The plan has no query block"))?;
    Ok(step("query_block", block, 0).unwrap_or_else(|| PlanNode::named("Query block")))
}

fn number(object: &Map<String, Json>, key: &str) -> Option<f64> {
    object.get(key).and_then(Json::as_f64)
}

fn text(object: &Map<String, Json>, key: &str) -> Option<String> {
    object.get(key).and_then(Json::as_str).map(str::to_owned)
}

/// The step a key stands for, or `None` for keys that only hold data.
fn step(key: &str, value: &Json, depth: usize) -> Option<PlanNode> {
    if depth > MAX_DEPTH {
        return None;
    }
    if let Json::Array(items) = value {
        let children = items
            .iter()
            .filter_map(|item| wrapped(item, depth + 1))
            .collect();
        return Some(with_children(PlanNode::named(label(key)?), children));
    }
    let object = value.as_object()?;
    let node = match key {
        "table" => table(object),
        "filesort" => PlanNode {
            details: text(object, "sort_key")
                .map(|k| format!("Sort key: {k}"))
                .into_iter()
                .collect(),
            actual_time_ms: number(object, "r_total_time_ms"),
            loops: number(object, "r_loops"),
            actual_rows: number(object, "r_output_rows"),
            ..PlanNode::named("Sort")
        },
        "query_block" => PlanNode {
            estimated_rows: None,
            cost: number(object, "cost"),
            actual_time_ms: number(object, "r_total_time_ms"),
            loops: number(object, "r_loops"),
            ..PlanNode::named("Query block")
        },
        other => PlanNode::named(label(other)?),
    };
    let children = object
        .iter()
        .filter_map(|(k, v)| step(k, v, depth + 1))
        .collect();
    Some(with_children(node, children))
}

/// An array element such as `{"table": {...}}` or `{"query_block": {...}}`.
fn wrapped(item: &Json, depth: usize) -> Option<PlanNode> {
    let object = item.as_object()?;
    let mut nodes: Vec<PlanNode> = object
        .iter()
        .filter_map(|(k, v)| step(k, v, depth))
        .collect();
    if nodes.len() == 1 {
        nodes.pop()
    } else {
        Some(with_children(PlanNode::named("Step"), nodes))
    }
}

/// Names of the keys that are steps; anything else is data.
fn label(key: &str) -> Option<&'static str> {
    Some(match key {
        "nested_loop" => "Nested loop",
        "temporary_table" => "Temporary table",
        "read_sorted_file" => "Read sorted file",
        "materialized" => "Materialized",
        "duplicates_removal" => "Duplicates removal",
        "subqueries" => "Subqueries",
        "union_result" => "Union",
        "query_specifications" => "Union parts",
        "query_block" => "Query block",
        "filesort" => "Sort",
        _ => return None,
    })
}

/// A step without a measured time of its own spends its children's.
fn with_children(node: PlanNode, children: Vec<PlanNode>) -> PlanNode {
    let summed: Option<f64> = children.iter().map(|c| c.actual_time_ms).sum();
    PlanNode {
        actual_time_ms: node.actual_time_ms.or(summed),
        children,
        ..node
    }
}

fn table(object: &Map<String, Json>) -> PlanNode {
    let name = text(object, "table_name");
    let access = text(object, "access_type").unwrap_or_else(|| "access".to_owned());
    let full_scan = access == "ALL";
    let operation = match (&name, text(object, "key")) {
        (Some(name), _) if full_scan => format!("Full scan on {name}"),
        (Some(name), Some(key)) => format!("{access} on {name} using {key}"),
        (Some(name), None) => format!("{access} on {name}"),
        (None, _) => access,
    };
    let read = number(object, "r_rows");
    let filtered = number(object, "r_filtered").unwrap_or(100.0) / 100.0;
    let own = [
        number(object, "r_table_time_ms"),
        number(object, "r_other_time_ms"),
    ];
    PlanNode {
        operation,
        relation: name.clone(),
        table: name,
        details: text(object, "attached_condition")
            .map(|c| format!("Condition: {c}"))
            .into_iter()
            .collect(),
        filter: text(object, "attached_condition"),
        // Rows kept, like `actual_rows`, so the two compare.
        estimated_rows: number(object, "rows")
            .map(|rows| rows * number(object, "filtered").unwrap_or(100.0) / 100.0),
        cost: number(object, "cost"),
        rows_read: read,
        actual_rows: read.map(|rows| rows * filtered),
        loops: number(object, "r_loops"),
        actual_time_ms: own.iter().flatten().copied().reduce(|a, b| a + b),
        full_scan,
        ..PlanNode::default()
    }
}
