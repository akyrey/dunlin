//! Query plans as a tree (deferred feature 3, visual `EXPLAIN`). Drivers read their server's
//! plan format into `PlanNode`; the builders here cover the shapes several servers share.

use serde::Serialize;

/// One step of a plan. Numbers are the server's estimates, in its own units.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlanNode {
    /// What the step does: "Seq Scan", "Index lookup on t using PRIMARY", "SCAN people".
    pub operation: String,
    /// The table it reads, when the server names one.
    pub relation: Option<String>,
    /// Conditions and other notes, one per line.
    pub details: Vec<String>,
    pub estimated_rows: Option<f64>,
    /// The server's total cost estimate for this step and everything under it.
    pub cost: Option<f64>,
    pub children: Vec<PlanNode>,
}

/// The statement to put after `EXPLAIN`: trimmed, without trailing semicolons.
pub fn statement_body(statement: &str) -> &str {
    statement.trim().trim_end_matches(';').trim_end()
}

/// The text of every row's column `index`, for plans returned as rows of text.
pub fn column_text(outcome: &crate::result::StatementOutcome, index: usize) -> Vec<String> {
    let crate::result::StatementOutcome::Rows(result) = outcome else {
        return Vec::new();
    };
    result
        .rows
        .iter()
        .filter_map(|row| match row.get(index)? {
            crate::Value::Text(text) | crate::Value::Json(text) => Some(text.clone()),
            crate::Value::Int(n) => Some(n.to_string()),
            crate::Value::Null => None,
            other => Some(format!("{other:?}")),
        })
        .collect()
}

/// The root shown when a server returns several top-level steps.
pub const QUERY: &str = "Query";

impl PlanNode {
    pub fn named(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            ..Self::default()
        }
    }

    /// `nodes` under one root: the node itself when there is exactly one.
    pub fn root(mut nodes: Vec<PlanNode>) -> PlanNode {
        if nodes.len() == 1
            && let Some(only) = nodes.pop()
        {
            return only;
        }
        PlanNode {
            children: nodes,
            ..PlanNode::named(QUERY)
        }
    }
}

/// A plan printed as an indented tree of `-> step  (cost=.. rows=..)` lines, as MySQL's
/// `EXPLAIN FORMAT=TREE` does. Lines without an arrow are details of the step above.
pub fn from_indented(text: &str) -> PlanNode {
    // (indent, node) for the open path from the root down.
    let mut stack: Vec<(usize, PlanNode)> = Vec::new();
    let mut roots = Vec::new();
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        let Some(step) = trimmed.strip_prefix("->") else {
            if let Some((_, node)) = stack.last_mut()
                && !trimmed.is_empty()
            {
                node.details.push(trimmed.to_owned());
            }
            continue;
        };
        close_deeper(&mut stack, &mut roots, indent);
        stack.push((indent, indented_step(step.trim())));
    }
    close_deeper(&mut stack, &mut roots, 0);
    PlanNode::root(roots)
}

/// Pops every open node at `indent` or deeper into its parent (or the roots).
fn close_deeper(stack: &mut Vec<(usize, PlanNode)>, roots: &mut Vec<PlanNode>, indent: usize) {
    while stack.last().is_some_and(|(open, _)| *open >= indent) {
        let Some((_, node)) = stack.pop() else { break };
        match stack.last_mut() {
            Some((_, parent)) => parent.children.push(node),
            None => roots.push(node),
        }
    }
}

/// `Table scan on t  (cost=0.35 rows=1)` → operation, relation, cost, rows.
fn indented_step(step: &str) -> PlanNode {
    let (label, estimates) = match step.find("  (cost=").or_else(|| step.find(" (cost=")) {
        Some(at) => (step[..at].trim(), &step[at..]),
        None => (step, ""),
    };
    let number = |key: &str| {
        let start = estimates.find(key)? + key.len();
        let rest = &estimates[start..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == 'e' || c == '+' || c == '-'))
            .unwrap_or(rest.len());
        rest[..end].parse::<f64>().ok()
    };
    let relation = [" on ", " scan on "]
        .iter()
        .find_map(|marker| label.rsplit_once(marker).map(|(_, r)| r))
        .and_then(|rest| rest.split_whitespace().next())
        .map(str::to_owned);
    PlanNode {
        operation: label.to_owned(),
        relation,
        details: Vec::new(),
        estimated_rows: number("rows="),
        cost: number("cost="),
        children: Vec::new(),
    }
}

/// One row of a plan given as `(id, parent id, text)`, as SQLite's `EXPLAIN QUERY PLAN` does.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanRow {
    pub id: i64,
    pub parent: i64,
    pub text: String,
}

/// Builds the tree from rows that name their parent; rows whose parent is not listed are
/// top-level. Order among siblings is the rows' order.
pub fn from_parent_rows(rows: &[PlanRow]) -> PlanNode {
    let known: std::collections::HashSet<i64> = rows.iter().map(|r| r.id).collect();
    let tops = rows
        .iter()
        .filter(|r| r.parent == r.id || !known.contains(&r.parent))
        .map(|r| subtree(rows, r, 0))
        .collect();
    PlanNode::root(tops)
}

/// Deep enough for any real plan; guards against rows that form a cycle.
const MAX_DEPTH: usize = 64;

fn subtree(rows: &[PlanRow], row: &PlanRow, depth: usize) -> PlanNode {
    let children = if depth < MAX_DEPTH {
        rows.iter()
            .filter(|r| r.parent == row.id && r.id != row.id)
            .map(|r| subtree(rows, r, depth + 1))
            .collect()
    } else {
        Vec::new()
    };
    let relation = row
        .text
        .split_whitespace()
        .skip_while(|w| !matches!(*w, "SCAN" | "SEARCH"))
        .nth(1)
        .map(str::to_owned);
    PlanNode {
        operation: row.text.clone(),
        relation,
        children,
        ..PlanNode::default()
    }
}

#[cfg(test)]
mod tests;
