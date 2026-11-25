//! A column definition edited clause by clause from its `SHOW CREATE TABLE` text (D-042).

use dbc_core::DbError;
use dbc_core::dialect::{Dialect, string_literal};
use dbc_core::schema::Column;
use dbc_core::schema_edit::ColumnDefinition;
use dbc_core::schema_edit::render::default_clause;
use dbc_core::sql::definition::{Clause, ColumnParts};

use super::refused;
use crate::MysqlDialect;

/// Clauses that belong to the old type and may not suit a new one.
const TYPE_CLAUSES: &[&str] = &["CHARACTER", "CHARSET", "COLLATE"];

fn is_nullability(clause: &Clause) -> bool {
    clause.keyword == "NULL"
        || (clause.keyword == "NOT" && clause.text.to_ascii_uppercase().starts_with("NOT NULL"))
}

/// A comment literal. MySQL reads backslashes in strings as escapes unless `sql_mode` says
/// otherwise, and `COMMENT` takes no hex literal, so a backslash cannot be written safely.
pub(crate) fn comment_literal(comment: &str) -> Result<String, DbError> {
    if comment.chars().any(|c| c == '\\' || c.is_control()) {
        return Err(refused(
            "A MySQL comment cannot contain a backslash or control characters",
        ));
    }
    Ok(string_literal(comment))
}

/// `` `name` type [NOT NULL] [DEFAULT x] [COMMENT 'c'] `` for a new column.
pub(crate) fn new_definition(column: &ColumnDefinition) -> Result<String, DbError> {
    let mut sql = format!(
        "{} {}",
        MysqlDialect.quote_identifier(&column.name),
        column.type_name
    );
    if !column.nullable {
        sql.push_str(" NOT NULL");
    }
    if let Some(default) = &column.default {
        sql.push(' ');
        sql.push_str(&default_clause(default));
    }
    if let Some(comment) = &column.comment {
        sql.push_str(" COMMENT ");
        sql.push_str(&comment_literal(comment)?);
    }
    Ok(sql)
}

/// Where a new clause goes: after the clauses that `before` lists, else at the end.
fn insert_after(clauses: Vec<Clause>, clause: Clause, before: &[&str]) -> Vec<Clause> {
    let at = clauses
        .iter()
        .rposition(|c| before.contains(&c.keyword.as_str()) || is_nullability(c))
        .map_or(0, |i| i + 1);
    let at = if before.is_empty() { clauses.len() } else { at };
    let mut out = clauses;
    out.insert(at, clause);
    out
}

/// Replaces the clause `matches` finds with `clause` (in place), or inserts it after the
/// clauses `after` lists; `None` removes it.
fn set_clause(
    clauses: Vec<Clause>,
    matches: impl Fn(&Clause) -> bool,
    clause: Option<Clause>,
    after: &[&str],
) -> Vec<Clause> {
    let Some(clause) = clause else {
        return clauses.into_iter().filter(|c| !matches(c)).collect();
    };
    match clauses.iter().position(&matches) {
        Some(at) => {
            let mut out = clauses;
            out[at] = clause;
            out
        }
        None => insert_after(clauses, clause, after),
    }
}

fn clause(keyword: &str, text: String) -> Clause {
    Clause {
        keyword: keyword.to_owned(),
        text,
    }
}

/// The column's server definition with only what the user changed replaced.
pub(crate) fn changed_definition(
    server: &ColumnParts,
    old: &Column,
    new: &ColumnDefinition,
) -> Result<ColumnParts, DbError> {
    let mut parts = ColumnParts {
        name: MysqlDialect.quote_identifier(&new.name),
        ..server.clone()
    };
    if new.type_name != old.type_name {
        parts.type_text.clone_from(&new.type_name);
        parts
            .clauses
            .retain(|c| !TYPE_CLAUSES.contains(&c.keyword.as_str()));
    }
    if new.nullable != old.nullable {
        let text = if new.nullable { "NULL" } else { "NOT NULL" };
        parts.clauses = set_clause(
            parts.clauses,
            is_nullability,
            Some(clause("NOT", text.to_owned())),
            TYPE_CLAUSES,
        );
    }
    if new.default != old.default {
        let default = new
            .default
            .as_deref()
            .map(|d| clause("DEFAULT", default_clause(d)));
        parts.clauses = set_clause(
            parts.clauses,
            |c| c.keyword == "DEFAULT",
            default,
            TYPE_CLAUSES,
        );
    }
    if new.default.is_none() && !new.nullable {
        // The server writes `DEFAULT NULL` for a nullable column without a default; a
        // `NOT NULL` column cannot keep it.
        parts
            .clauses
            .retain(|c| !c.text.eq_ignore_ascii_case("DEFAULT NULL"));
    }
    if new.comment != old.comment {
        let comment = match new.comment.as_deref() {
            Some(c) => Some(clause(
                "COMMENT",
                format!("COMMENT {}", comment_literal(c)?),
            )),
            None => None,
        };
        parts.clauses = set_clause(parts.clauses, |c| c.keyword == "COMMENT", comment, &[]);
    }
    Ok(parts)
}
