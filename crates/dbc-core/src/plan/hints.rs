//! Hints on a plan (task 10.7, D-043), computed the same way for every server from the fields
//! each driver fills in `PlanNode`.

use serde::Serialize;

use super::PlanNode;
use super::filter::single_column;
use crate::sql::SplitterRules;

/// A step's rows must differ from the estimate by this factor, either way, to be a misestimate.
pub const MISESTIMATE_FACTOR: f64 = 10.0;
/// Below this many rows (estimated and actual), a misestimate does not matter.
pub const MISESTIMATE_MIN_ROWS: f64 = 100.0;
/// A full scan reading at least this many rows per loop is worth a hint.
pub const FULL_SCAN_ROWS: f64 = 10_000.0;
/// A step run at least this many times is worth a hint.
pub const MANY_LOOPS: f64 = 1_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum HintKind {
    Hottest,
    Misestimate,
    FullScan,
    Spill,
    ManyLoops,
}

/// A one-column index that would serve a full scan's filter. The backend checks the table
/// exists and has no index starting with the column before showing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct IndexSuggestion {
    pub table: String,
    pub column: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlanHint {
    /// Child indexes from the root to the step; empty for the root.
    pub path: Vec<usize>,
    pub kind: HintKind,
    pub message: String,
    pub suggestion: Option<IndexSuggestion>,
}

/// Whether a suggested one-column index is new: the column exists and no index or key of the
/// table already starts with it.
pub fn index_is_missing(info: &crate::schema::TableInfo, column: &str) -> bool {
    let starts = |columns: &[String]| columns.first().is_some_and(|c| c == column);
    info.column_named(column).is_some()
        && !info.indexes.iter().any(|i| starts(&i.columns))
        && !info
            .primary_key
            .as_ref()
            .is_some_and(|k| starts(&k.columns))
        && !info.unique_constraints.iter().any(|k| starts(&k.columns))
}

/// Every hint for `plan`. `rules` are the dialect's, for reading filter text.
pub fn hints(plan: &PlanNode, rules: SplitterRules) -> Vec<PlanHint> {
    let mut found = hottest(plan).into_iter().collect::<Vec<_>>();
    walk(plan, None, &mut Vec::new(), rules, &mut found);
    found
}

fn hint(path: &[usize], kind: HintKind, message: String) -> PlanHint {
    PlanHint {
        path: path.to_vec(),
        kind,
        message,
        suggestion: None,
    }
}

fn walk(
    node: &PlanNode,
    parent: Option<&PlanNode>,
    path: &mut Vec<usize>,
    rules: SplitterRules,
    found: &mut Vec<PlanHint>,
) {
    if let Some(found_hint) = misestimate(node, path) {
        found.push(found_hint);
    }
    if let Some(found_hint) = full_scan(node, parent, path, rules) {
        found.push(found_hint);
    }
    if node.spilled {
        found.push(hint(path, HintKind::Spill, "This step ran out of memory and wrote to disk; more work memory or fewer rows would help.".into()));
    }
    if !path.is_empty() && node.loops.is_some_and(|loops| loops >= MANY_LOOPS) {
        let loops = node.loops.unwrap_or_default();
        found.push(hint(
            path,
            HintKind::ManyLoops,
            format!("This step ran {loops:.0} times, once per row of the step that drives it."),
        ));
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        walk(child, Some(node), path, rules, found);
        path.pop();
    }
}

/// Time measured anywhere means times are compared; otherwise costs.
fn measure(node: &PlanNode, timed: bool) -> Option<f64> {
    if timed {
        node.actual_time_ms
    } else {
        node.cost
    }
}

fn any_timed(node: &PlanNode) -> bool {
    node.actual_time_ms.is_some() || node.children.iter().any(any_timed)
}

/// The step with the most time (or cost) of its own: its total minus its children's.
fn hottest(plan: &PlanNode) -> Option<PlanHint> {
    if plan.children.is_empty() {
        return None;
    }
    let timed = any_timed(plan);
    let total = measure(plan, timed)?;
    let mut best: Option<(Vec<usize>, f64)> = None;
    let mut stack = vec![(Vec::new(), plan)];
    while let Some((path, node)) = stack.pop() {
        let inclusive = measure(node, timed).unwrap_or(0.0);
        let children: f64 = node.children.iter().filter_map(|c| measure(c, timed)).sum();
        let own = (inclusive - children).max(0.0);
        if best.as_ref().is_none_or(|(_, b)| own > *b) {
            best = Some((path.clone(), own));
        }
        for (i, child) in node.children.iter().enumerate() {
            stack.push(([path.as_slice(), &[i]].concat(), child));
        }
    }
    let (path, own) = best.filter(|(_, own)| *own > 0.0)?;
    let share = if total > 0.0 {
        own / total * 100.0
    } else {
        0.0
    };
    let what = if timed { "time" } else { "estimated cost" };
    Some(hint(
        &path,
        HintKind::Hottest,
        format!("The most {what} is spent here: {share:.0}% of the total."),
    ))
}

fn misestimate(node: &PlanNode, path: &[usize]) -> Option<PlanHint> {
    let (estimated, actual) = (node.estimated_rows?, node.actual_rows?);
    if estimated.max(actual) < MISESTIMATE_MIN_ROWS {
        return None;
    }
    let ratio = estimated.max(actual) / estimated.min(actual).max(1.0);
    (ratio >= MISESTIMATE_FACTOR).then(|| {
        hint(path, HintKind::Misestimate, format!(
            "The planner expected {estimated:.0} rows here and got {actual:.0}. Outdated statistics are the usual cause; analyzing the table updates them."
        ))
    })
}

fn full_scan(
    node: &PlanNode,
    parent: Option<&PlanNode>,
    path: &[usize],
    rules: SplitterRules,
) -> Option<PlanHint> {
    if !node.full_scan {
        return None;
    }
    let rows = node
        .rows_read
        .or(node.actual_rows)
        .or(node.estimated_rows)?;
    if rows < FULL_SCAN_ROWS {
        return None;
    }
    let table = node.table.clone()?;
    // MySQL shows the filter as its own step above the scan.
    let parent_filter = parent
        .filter(|p| p.children.len() == 1)
        .and_then(|p| p.filter.as_deref());
    let filter = node.filter.as_deref().or(parent_filter);
    let alias = node
        .relation
        .as_deref()
        .and_then(|r| r.split_whitespace().nth(1));
    let qualifiers: Vec<&str> = [Some(table.as_str()), alias]
        .into_iter()
        .flatten()
        .collect();
    let suggestion = filter
        .and_then(|f| single_column(f, &qualifiers, rules))
        .map(|column| IndexSuggestion {
            table: table.clone(),
            column,
        });
    Some(PlanHint {
        suggestion,
        ..hint(
            path,
            HintKind::FullScan,
            format!("Reads every row of {table} (about {rows:.0})."),
        )
    })
}

#[cfg(test)]
#[path = "hints_tests.rs"]
mod tests;
