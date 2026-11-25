//! Clauses every planner writes the same way, apart from quoting.

use crate::dialect::Dialect;
use crate::schema::{Check, ForeignKey, Key, TableInfo};

/// `"a", "b"`
pub fn column_list(dialect: &dyn Dialect, columns: &[String]) -> String {
    columns
        .iter()
        .map(|c| dialect.quote_identifier(c))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `CONSTRAINT "name" `, or nothing for an unnamed constraint.
pub fn constraint_name(dialect: &dyn Dialect, name: Option<&str>) -> String {
    name.filter(|n| !n.is_empty())
        .map(|n| format!("CONSTRAINT {} ", dialect.quote_identifier(n)))
        .unwrap_or_default()
}

pub fn primary_key_clause(dialect: &dyn Dialect, key: &Key) -> String {
    format!(
        "{}PRIMARY KEY ({})",
        constraint_name(dialect, key.name.as_deref()),
        column_list(dialect, &key.columns)
    )
}

pub fn unique_clause(dialect: &dyn Dialect, key: &Key) -> String {
    format!(
        "{}UNIQUE ({})",
        constraint_name(dialect, key.name.as_deref()),
        column_list(dialect, &key.columns)
    )
}

pub fn check_clause(dialect: &dyn Dialect, check: &Check) -> String {
    format!(
        "{}CHECK ({})",
        constraint_name(dialect, check.name.as_deref()),
        check.expression
    )
}

pub fn foreign_key_clause(dialect: &dyn Dialect, fk: &ForeignKey) -> String {
    let mut clause = format!(
        "{}FOREIGN KEY ({}) REFERENCES {} ({})",
        constraint_name(dialect, fk.name.as_deref()),
        column_list(dialect, &fk.columns),
        dialect.qualified_name(&fk.referenced_table),
        column_list(dialect, &fk.referenced_columns)
    );
    if let Some(action) = fk.on_update {
        clause.push_str(" ON UPDATE ");
        clause.push_str(action.as_sql());
    }
    if let Some(action) = fk.on_delete {
        clause.push_str(" ON DELETE ");
        clause.push_str(action.as_sql());
    }
    clause
}

/// Introspection reports identity and generated columns through `Column::default`
/// (DRIVER_API.md#schema-model).
pub fn is_identity_or_generated(default: &str) -> bool {
    let upper = default.trim_start().to_ascii_uppercase();
    upper.starts_with("GENERATED ") || upper.starts_with("AS ")
}

/// `DEFAULT <expression>`, or an identity clause as written.
pub fn default_clause(default: &str) -> String {
    if is_identity_or_generated(default) {
        default.trim().to_owned()
    } else {
        format!("DEFAULT {default}")
    }
}

/// An index key part: a quoted column when it names one, else the expression in parentheses.
pub fn index_part(dialect: &dyn Dialect, info: &TableInfo, part: &str) -> String {
    if info.columns.iter().any(|c| c.name == part) {
        dialect.quote_identifier(part)
    } else {
        format!("({part})")
    }
}

/// `"a", (lower("b"))`
pub fn index_parts(dialect: &dyn Dialect, info: &TableInfo, parts: &[String]) -> String {
    parts
        .iter()
        .map(|p| index_part(dialect, info, p))
        .collect::<Vec<_>>()
        .join(", ")
}
