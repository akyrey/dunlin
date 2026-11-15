//! Redis (and Valkey) driver: the editor is a command console, and the key browser reads keys
//! by type (D-032). Single servers only; Cluster and Sentinel are not supported.

mod command;
mod connection;
mod errors;
mod reply;
mod tls;

use std::sync::Arc;

use async_trait::async_trait;
use dbc_core::DbError;
use dbc_core::dialect::Dialect;
use dbc_core::driver::{
    Capabilities, Connection, ConnectionConfig, Driver, DriverInfo, DriverKind, EndpointKind,
    TlsMode,
};
use dbc_core::sql::{Classification, SplitterRules, StatementRange};

pub use command::{parse_args, quote_arg};
pub use connection::{RedisConnection, database_index};

pub const DEFAULT_PORT: u16 = 6379;

#[derive(Debug, Default, Clone, Copy)]
pub struct RedisDriver;

#[async_trait]
impl Driver for RedisDriver {
    fn info(&self) -> DriverInfo {
        DriverInfo {
            kind: DriverKind::Redis,
            label: "Redis / Valkey".to_owned(),
            endpoint: EndpointKind::Tcp,
            default_port: Some(DEFAULT_PORT),
            // No `prefer`: Redis does not negotiate TLS, a port speaks it or not.
            tls_modes: vec![
                TlsMode::Disable,
                TlsMode::Require,
                TlsMode::VerifyCa,
                TlsMode::VerifyFull,
            ],
            capabilities: Capabilities {
                schemas: false,
                multiple_databases: false,
                cancel: true,
                read_only_session: false,
                transactional_ddl: false,
                explain: false,
                tls: true,
                tunnel: true,
                key_value: true,
            },
            options: Vec::new(),
        }
    }

    fn dialect(&self) -> Arc<dyn Dialect> {
        Arc::new(RedisDialect)
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<Arc<dyn Connection>, DbError> {
        Ok(Arc::new(RedisConnection::open(config).await?))
    }
}

/// Redis commands: one per line, redis-cli quoting.
#[derive(Debug, Default, Clone, Copy)]
pub struct RedisDialect;

impl Dialect for RedisDialect {
    fn quote_identifier(&self, name: &str) -> String {
        quote_arg(name)
    }

    fn splitter_rules(&self) -> SplitterRules {
        SplitterRules::default()
    }

    fn split(&self, text: &str) -> Vec<StatementRange> {
        command::split_lines(text)
    }

    fn classify(&self, statement: &str) -> Classification {
        command::classify(statement)
    }

    /// Redis has no SQL parameters; nothing in the app builds SQL for a key-value driver.
    fn placeholder(&self, _index: usize, _type_name: &str) -> String {
        "?".to_owned()
    }
}
