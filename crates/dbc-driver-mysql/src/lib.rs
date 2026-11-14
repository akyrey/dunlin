//! MySQL and MariaDB driver.

mod connection;
mod decode;
mod errors;
mod introspect;

use std::sync::Arc;

use async_trait::async_trait;
use dbc_core::DbError;
use dbc_core::dialect::{Dialect, quote_with};
use dbc_core::driver::{
    Capabilities, Connection, ConnectionConfig, Driver, DriverInfo, DriverKind, EndpointKind,
    TlsMode,
};
use dbc_core::sql::SplitterRules;

pub const DEFAULT_PORT: u16 = 3306;

#[derive(Debug, Default, Clone, Copy)]
pub struct MysqlDriver;

#[async_trait]
impl Driver for MysqlDriver {
    fn info(&self) -> DriverInfo {
        DriverInfo {
            kind: DriverKind::Mysql,
            label: "MySQL / MariaDB".to_owned(),
            endpoint: EndpointKind::Tcp,
            default_port: Some(DEFAULT_PORT),
            // `verify-ca` is not offered: mysql_async 0.37 cannot skip only the name check (D-018).
            tls_modes: vec![
                TlsMode::Disable,
                TlsMode::Prefer,
                TlsMode::Require,
                TlsMode::VerifyFull,
            ],
            capabilities: Capabilities {
                schemas: false,
                multiple_databases: true,
                cancel: true,
                read_only_session: true,
                transactional_ddl: false,
                explain: true,
                tls: true,
                tunnel: true,
            },
            options: Vec::new(),
        }
    }

    fn dialect(&self) -> Arc<dyn Dialect> {
        Arc::new(MysqlDialect)
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<Arc<dyn Connection>, DbError> {
        Ok(Arc::new(connection::MysqlConnection::open(config).await?))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct MysqlDialect;

impl Dialect for MysqlDialect {
    fn quote_identifier(&self, name: &str) -> String {
        quote_with(name, '`')
    }

    fn splitter_rules(&self) -> SplitterRules {
        rules()
    }
}

fn rules() -> SplitterRules {
    SplitterRules {
        backtick_identifiers: true,
        hash_comments: true,
        backslash_escapes: true,
        ..SplitterRules::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_with_backticks() {
        assert_eq!(MysqlDialect.quote_identifier("a`b"), "`a``b`");
    }

    #[test]
    fn info_offers_every_tls_mode_except_verify_ca() {
        let info = MysqlDriver.info();

        assert_eq!(info.kind, DriverKind::Mysql);
        assert_eq!(info.default_port, Some(DEFAULT_PORT));
        assert!(!info.tls_modes.contains(&TlsMode::VerifyCa), "D-018");
        assert!(info.tls_modes.contains(&TlsMode::VerifyFull));
        assert!(info.capabilities.multiple_databases && !info.capabilities.schemas);
    }

    #[test]
    fn splitter_rules_follow_mysql_lexing() {
        let rules = MysqlDriver.dialect().splitter_rules();

        assert!(rules.backtick_identifiers && rules.hash_comments && rules.backslash_escapes);
        assert!(!rules.dollar_quoting);
    }
}
