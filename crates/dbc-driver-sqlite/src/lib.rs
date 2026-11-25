//! SQLite driver. Each connection lives on its own worker thread (`rusqlite` is not `Sync`).

mod completion;
mod ddl;
mod decode;
mod errors;
mod introspect;
mod schema_edit;
mod worker;

pub use schema_edit::{TableSource, plan_schema_edit};

use std::sync::Arc;

use async_trait::async_trait;
use dbc_core::DbError;
use dbc_core::Value;
use dbc_core::dialect::{Dialect, ansi_literal, quote_with, string_literal};
use dbc_core::driver::{
    Capabilities, Connection, ConnectionConfig, Driver, DriverInfo, DriverKind, EndpointKind,
    OptionField, OptionKind,
};
use dbc_core::sql::SplitterRules;

/// Driver option: create the database file when it does not exist.
pub const OPTION_CREATE_IF_MISSING: &str = "create_if_missing";

#[derive(Debug, Default, Clone, Copy)]
pub struct SqliteDriver;

#[async_trait]
impl Driver for SqliteDriver {
    fn info(&self) -> DriverInfo {
        DriverInfo {
            kind: DriverKind::Sqlite,
            label: "SQLite".to_owned(),
            endpoint: EndpointKind::File,
            default_port: None,
            tls_modes: Vec::new(),
            capabilities: Capabilities {
                cancel: true,
                read_only_session: true,
                transactional_ddl: true,
                explain: true,
                schema_editing: false,
                ..Capabilities::default()
            },
            options: vec![OptionField {
                key: OPTION_CREATE_IF_MISSING.to_owned(),
                label: "Create the file if it does not exist".to_owned(),
                kind: OptionKind::Bool,
                default: Some("false".to_owned()),
                help: None,
            }],
        }
    }

    fn dialect(&self) -> Arc<dyn Dialect> {
        Arc::new(SqliteDialect)
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<Arc<dyn Connection>, DbError> {
        let connection = worker::SqliteConnection::open(config).await?;
        Ok(Arc::new(connection))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SqliteDialect;

impl Dialect for SqliteDialect {
    fn quote_identifier(&self, name: &str) -> String {
        quote_with(name, '"')
    }

    fn splitter_rules(&self) -> SplitterRules {
        SplitterRules {
            backtick_identifiers: true,
            bracket_identifiers: true,
            ..SplitterRules::default()
        }
    }

    fn placeholder(&self, index: usize, _type_name: &str) -> String {
        format!("?{index}")
    }

    /// Decimals are bound as text, so their literal is quoted too: an unquoted
    /// `12345678901234567890.123456789` would become a lossy float.
    fn render_literal(&self, value: &Value, _type_name: &str) -> String {
        match value {
            Value::Decimal(text) => string_literal(text),
            other => ansi_literal(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_quotes_with_double_quotes() {
        assert_eq!(SqliteDialect.quote_identifier("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn info_declares_a_file_endpoint_without_tunnel_or_tls() {
        let info = SqliteDriver.info();

        assert_eq!(info.endpoint, EndpointKind::File);
        assert!(!info.capabilities.tunnel && !info.capabilities.tls);
        assert_eq!(info.options[0].key, OPTION_CREATE_IF_MISSING);
    }
}
