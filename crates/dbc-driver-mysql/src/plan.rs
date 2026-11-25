//! MySQL plans: `EXPLAIN FORMAT=TREE` (MySQL 8.0.16+) as an indented tree, or the classic
//! tabular `EXPLAIN` (MariaDB, older MySQL) as one step per table.

use dbc_core::Value;
use dbc_core::plan::PlanNode;
use dbc_core::result::ResultSet;

/// One node per row of the classic `EXPLAIN`, under a query root.
pub(crate) fn from_table(result: &ResultSet) -> PlanNode {
    let column = |name: &str| {
        result
            .columns
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(name))
    };
    let (table, access, key, rows, extra) = (
        column("table"),
        column("type"),
        column("key"),
        column("rows"),
        column("Extra"),
    );
    let text = |row: &[Value], at: Option<usize>| -> Option<String> {
        match row.get(at?)? {
            Value::Null => None,
            Value::Text(t) => Some(t.clone()),
            Value::Int(n) => Some(n.to_string()),
            Value::UInt(n) => Some(n.to_string()),
            other => Some(format!("{other:?}")),
        }
    };
    let nodes = result
        .rows
        .iter()
        .map(|row| {
            let access = text(row, access).unwrap_or_default();
            let relation = text(row, table);
            let full_scan = access == "ALL" && relation.is_some();
            PlanNode {
                full_scan,
                table: relation.clone(),
                operation: match (access.as_str(), &relation) {
                    ("ALL", Some(_)) => "Table scan".to_owned(),
                    ("", _) => "Step".to_owned(),
                    (other, _) => format!("Access: {other}"),
                },
                relation,
                details: [
                    text(row, key).map(|k| format!("Key: {k}")),
                    text(row, extra).map(|e| format!("Extra: {e}")),
                ]
                .into_iter()
                .flatten()
                .collect(),
                estimated_rows: text(row, rows).and_then(|r| r.parse().ok()),
                ..PlanNode::default()
            }
        })
        .collect();
    PlanNode {
        children: nodes,
        ..PlanNode::named(dbc_core::plan::QUERY)
    }
}

#[cfg(test)]
mod tests {
    use dbc_core::result::{ColumnMeta, TypeCategory};

    use super::*;

    fn column(name: &str) -> ColumnMeta {
        ColumnMeta {
            name: name.into(),
            type_name: String::new(),
            category: TypeCategory::Text,
            nullable: None,
        }
    }

    #[test]
    fn each_table_row_is_a_step_with_its_access_and_estimate() {
        let result = ResultSet {
            columns: ["id", "table", "type", "key", "rows", "Extra"]
                .map(column)
                .to_vec(),
            rows: vec![
                vec![
                    Value::Int(1),
                    Value::Text("p".into()),
                    Value::Text("ALL".into()),
                    Value::Null,
                    Value::UInt(5),
                    Value::Text("Using where".into()),
                ],
                vec![
                    Value::Int(1),
                    Value::Text("o".into()),
                    Value::Text("eq_ref".into()),
                    Value::Text("PRIMARY".into()),
                    Value::UInt(1),
                    Value::Null,
                ],
            ],
            truncated: false,
        };

        let plan = from_table(&result);

        assert_eq!(plan.children.len(), 2);
        assert_eq!(plan.children[0].operation, "Table scan");
        assert_eq!(plan.children[0].estimated_rows, Some(5.0));
        assert_eq!(plan.children[0].details, ["Extra: Using where"]);
        assert_eq!(plan.children[1].operation, "Access: eq_ref");
        assert_eq!(plan.children[1].details, ["Key: PRIMARY"]);
    }
}
