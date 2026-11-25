//! Redis (and Valkey) driver: the editor is a command console, and the key browser reads keys
//! by type (D-032). Single servers only; Cluster and Sentinel are not supported.

mod command;
mod connection;
mod edit;
mod errors;
mod kv;
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
                explain_analyze: false,
                explain_analyze_writes: false,
                schema_editing: false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use dbc_core::sql::StatementClass;

    #[test]
    fn the_driver_describes_a_key_value_store() {
        let info = RedisDriver.info();

        assert_eq!(info.kind, DriverKind::Redis);
        assert_eq!(info.default_port, Some(DEFAULT_PORT));
        assert!(info.capabilities.key_value);
        assert!(!info.capabilities.explain);
        assert!(
            !info.tls_modes.contains(&TlsMode::Prefer),
            "a port speaks TLS or not"
        );
    }

    #[test]
    fn the_dialect_reads_commands_line_by_line() {
        let dialect = RedisDriver.dialect();

        assert_eq!(dialect.split("GET a\n# note\nSET b 1").len(), 2);
        assert_eq!(dialect.classify("GET a").class, StatementClass::Read);
        assert_eq!(dialect.classify("SET a 1").class, StatementClass::Write);
        assert_eq!(dialect.quote_identifier("a key"), quote_arg("a key"));
        assert_eq!(dialect.placeholder(1, "text"), "?");
        assert!(dialect.splitter_rules() == SplitterRules::default());
    }
}
