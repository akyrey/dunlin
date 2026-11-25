//! PostgreSQL driver.

mod completion;
mod connection;
mod ddl;
mod decode;
mod errors;
mod introspect;
mod params;
mod plan;
mod raw;
mod schema_edit;
mod tls;

pub use schema_edit::plan_schema_edit;

/// `EXPLAIN (FORMAT JSON)` output, measured or not, as a plan tree.
pub use plan::{analyzed_from_json as plan_analyzed_from_json, from_json as plan_from_json};

use std::sync::Arc;

use async_trait::async_trait;
use dbc_core::DbError;
use dbc_core::Value;
use dbc_core::dialect::{Dialect, ansi_literal, hex, numeric_literal, quote_with, string_literal};
use dbc_core::driver::{
    Capabilities, Connection, ConnectionConfig, Driver, DriverInfo, DriverKind, EndpointKind,
    TlsMode,
};
use dbc_core::sql::SplitterRules;

pub const DEFAULT_PORT: u16 = 5432;

#[derive(Debug, Default, Clone, Copy)]
pub struct PostgresDriver;

#[async_trait]
impl Driver for PostgresDriver {
    fn info(&self) -> DriverInfo {
        DriverInfo {
            kind: DriverKind::Postgres,
            label: "PostgreSQL".to_owned(),
            endpoint: EndpointKind::Tcp,
            default_port: Some(DEFAULT_PORT),
            tls_modes: TlsMode::ALL.to_vec(),
            capabilities: Capabilities {
                schemas: true,
                multiple_databases: false,
                cancel: true,
                read_only_session: true,
                transactional_ddl: true,
                explain: true,
                explain_analyze: true,
                explain_analyze_writes: true,
                schema_editing: true,
                tls: true,
                tunnel: true,
                key_value: false,
            },
            options: Vec::new(),
        }
    }

    fn dialect(&self) -> Arc<dyn Dialect> {
        Arc::new(PostgresDialect)
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<Arc<dyn Connection>, DbError> {
        Ok(Arc::new(
            connection::PostgresConnection::open(config).await?,
        ))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PostgresDialect;

impl Dialect for PostgresDialect {
    fn quote_identifier(&self, name: &str) -> String {
        quote_with(name, '"')
    }

    fn fold_identifier(&self, name: &str) -> String {
        name.to_lowercase()
    }

    fn splitter_rules(&self) -> SplitterRules {
        SplitterRules {
            dollar_quoting: true,
            nested_block_comments: true,
            ..SplitterRules::default()
        }
    }

    /// Strings cast to the column's type, so the literal means what the bound parameter does.
    fn render_literal(&self, value: &Value, type_name: &str) -> String {
        let cast = |literal: String| {
            let plain_text = type_name.is_empty()
                || type_name == "text"
                || type_name.starts_with("character")
                || type_name.starts_with("varchar");
            if plain_text {
                literal
            } else {
                format!("{literal}::{type_name}")
            }
        };
        match value {
            Value::Bytes(bytes) => format!("'\\x{}'::bytea", hex(bytes)),
            Value::Float(f) if !f.is_finite() => {
                format!("{}::float8", string_literal(&f.to_string()))
            }
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::UInt(_) | Value::Float(_) => {
                ansi_literal(value)
            }
            Value::Decimal(text) if numeric_literal(text).is_some() => text.clone(),
            Value::Array(items) => cast(format!(
                "ARRAY[{}]",
                items
                    .iter()
                    .map(ansi_literal)
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            other => cast(ansi_literal(other)),
        }
    }

    /// Sent as text and cast, so every type binds the same way (`params.rs`).
    fn placeholder(&self, index: usize, type_name: &str) -> String {
        if type_name.is_empty() {
            format!("${index}")
        } else {
            format!("${index}::text::{type_name}")
        }
    }
}

/// Decodes every column of a row. Public for the decoding tests.
#[doc(hidden)]
pub fn decode_row(row: &tokio_postgres::Row) -> Vec<dbc_core::Value> {
    row.columns()
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let cell = row
                .try_get::<_, raw::RawCell<'_>>(index)
                .ok()
                .and_then(|c| c.0);
            decode::decode(column.type_(), cell)
        })
        .collect()
}

#[cfg(test)]
mod dialect_tests {
    use dbc_core::dialect::Dialect;

    use super::PostgresDialect;

    #[test]
    fn unquoted_identifiers_fold_to_lowercase() {
        assert_eq!(PostgresDialect.fold_identifier("People"), "people");
    }
}
