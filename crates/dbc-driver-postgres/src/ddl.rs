//! `CREATE` statements rebuilt from the catalogs: PostgreSQL has no `SHOW CREATE TABLE`.

use dbc_core::DbError;
use dbc_core::dialect::quote_with;
use dbc_core::schema::TableRef;
use tokio_postgres::Client;

use crate::introspect::{self, ColumnRow, ConstraintRow, IndexRow, get, query};

pub(crate) const HEADER: &str =
    "-- Reconstructed from the system catalogs; it may differ from the original statement.\n";

fn ident(name: &str) -> String {
    quote_with(name, '"')
}

/// A string literal; standard-conforming strings are the default since PostgreSQL 9.1.
fn literal(text: &str) -> String {
    quote_with(text, '\'')
}

pub(crate) async fn table_ddl(client: &Client, table: &TableRef) -> Result<String, DbError> {
    let relation = introspect::relation(client, table).await?;
    let schema = table.namespace.schema.as_deref().unwrap_or("public");
    let name = format!("{}.{}", ident(schema), ident(&table.name));
    let indexes = introspect::indexes(client, relation.oid).await?;
    let mut out = String::from(HEADER);
    match relation.relkind.as_str() {
        "v" | "m" => out.push_str(&view(client, relation.oid, &relation.relkind, &name).await?),
        kind => {
            let columns = introspect::columns(client, relation.oid).await?;
            let constraints = introspect::constraints(client, relation.oid).await?;
            out.push_str(&create_table(&name, &columns, &constraints));
            if kind == "p" {
                let rows = query(
                    client,
                    "SELECT pg_get_partkeydef($1::oid)",
                    &[&relation.oid],
                )
                .await?;
                if let Some(row) = rows.first() {
                    out.push_str(&format!(" PARTITION BY {}", get::<String>(row, 0)?));
                }
            }
            out.push_str(";\n");
            out.push_str(&comments(
                &name,
                relation.comment.as_deref(),
                &columns,
                &relation.relkind,
            ));
        }
    }
    out.push_str(&index_statements(&indexes));
    if matches!(relation.relkind.as_str(), "v" | "m") {
        let columns = introspect::columns(client, relation.oid).await?;
        out.push_str(&comments(
            &name,
            relation.comment.as_deref(),
            &columns,
            &relation.relkind,
        ));
    }
    Ok(out)
}

async fn view(
    client: &Client,
    oid: tokio_postgres::types::Oid,
    relkind: &str,
    name: &str,
) -> Result<String, DbError> {
    let rows = query(client, "SELECT pg_get_viewdef($1::oid, true)", &[&oid]).await?;
    let definition: String = match rows.first() {
        Some(row) => get(row, 0)?,
        None => String::new(),
    };
    let kind = if relkind == "m" {
        "MATERIALIZED VIEW"
    } else {
        "VIEW"
    };
    let body = definition.trim().trim_end_matches(';');
    Ok(format!("CREATE {kind} {name} AS\n{body};\n"))
}

fn create_table(name: &str, columns: &[ColumnRow], constraints: &[ConstraintRow]) -> String {
    let lines: Vec<String> = columns
        .iter()
        .map(column_line)
        .chain(
            constraints
                .iter()
                .map(|c| format!("CONSTRAINT {} {}", ident(&c.name), c.definition)),
        )
        .map(|line| format!("    {line}"))
        .collect();
    format!("CREATE TABLE {name} (\n{}\n)", lines.join(",\n"))
}

fn column_line(row: &ColumnRow) -> String {
    let column = &row.column;
    let mut line = format!("{} {}", ident(&column.name), column.type_name);
    match (row.generated.as_str(), row.identity.as_str()) {
        ("s" | "v", _) => {
            let storage = if row.generated == "s" {
                "STORED"
            } else {
                "VIRTUAL"
            };
            let expression = column.default.as_deref().unwrap_or_default();
            line.push_str(&format!(" GENERATED ALWAYS AS ({expression}) {storage}"));
        }
        (_, "a" | "d") => {
            line.push(' ');
            line.push_str(column.default.as_deref().unwrap_or_default());
        }
        _ => {
            if let Some(default) = &column.default {
                line.push_str(&format!(" DEFAULT {default}"));
            }
        }
    }
    if !column.nullable {
        line.push_str(" NOT NULL");
    }
    line
}

fn index_statements(indexes: &[IndexRow]) -> String {
    indexes
        .iter()
        .map(|index| format!("\n{};\n", index.definition))
        .collect()
}

fn comments(name: &str, table: Option<&str>, columns: &[ColumnRow], relkind: &str) -> String {
    let object = match relkind {
        "v" => "VIEW",
        "m" => "MATERIALIZED VIEW",
        "f" => "FOREIGN TABLE",
        _ => "TABLE",
    };
    let mut out = String::new();
    if let Some(comment) = table {
        out.push_str(&format!(
            "\nCOMMENT ON {object} {name} IS {};\n",
            literal(comment)
        ));
    }
    for row in columns {
        if let Some(comment) = &row.column.comment {
            out.push_str(&format!(
                "\nCOMMENT ON COLUMN {name}.{} IS {};\n",
                ident(&row.column.name),
                literal(comment)
            ));
        }
    }
    out
}
