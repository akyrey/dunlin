//! PostgreSQL driver.

mod connection;
mod ddl;
mod decode;
mod errors;
mod introspect;
mod raw;
mod tls;

use std::sync::Arc;

use async_trait::async_trait;
use dbc_core::DbError;
use dbc_core::dialect::{Dialect, quote_with};
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
                tls: true,
                tunnel: true,
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

    fn splitter_rules(&self) -> SplitterRules {
        SplitterRules {
            dollar_quoting: true,
            nested_block_comments: true,
            ..SplitterRules::default()
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
