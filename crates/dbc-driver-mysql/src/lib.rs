//! MySQL and MariaDB driver.

mod completion;
mod connection;
mod decode;
mod errors;
mod introspect;
mod plan;

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
                key_value: false,
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

    fn placeholder(&self, _index: usize, _type_name: &str) -> String {
        "?".to_owned()
    }

    /// Strings that backslash escaping could change (backslashes, control characters) are
    /// written as `_utf8mb4 X'..'`, which means the same in every `sql_mode`.
    fn render_literal(&self, value: &Value, _type_name: &str) -> String {
        let text = match value {
            Value::Timestamp(s) | Value::TimestampTz(s) => s.replacen('T', " ", 1),
            Value::Text(s)
            | Value::Json(s)
            | Value::Xml(s)
            | Value::Uuid(s)
            | Value::Date(s)
            | Value::Time(s)
            | Value::Interval(s)
            | Value::Decimal(s) => s.clone(),
            Value::Unknown { text: Some(s), .. } => s.clone(),
            other => return ansi_literal(other),
        };
        if let Value::Decimal(_) = value
            && numeric_literal(&text).is_some()
        {
            return text;
        }
        if text.chars().any(|c| c == '\\' || c.is_control()) {
            format!("_utf8mb4 X'{}'", hex(text.as_bytes()))
        } else {
            string_literal(&text)
        }
    }

    fn default_values(&self) -> &'static str {
        "() VALUES ()"
    }
}

fn rules() -> SplitterRules {
    SplitterRules {
        backtick_identifiers: true,
        hash_comments: true,
        backslash_escapes: true,
        delimiter_command: true,
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
