//! Driver and connection traits, and connection configuration.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use crate::dialect::Dialect;
use crate::error::DbError;
use crate::result::{ExecuteOptions, StatementOutcome, StreamEvent};
use crate::schema::{
    CompletionSchema, Namespace, NamespaceRef, ObjectSummary, TableInfo, TableRef,
};
use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum DriverKind {
    Postgres,
    Mysql,
    Sqlite,
}

impl DriverKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Mysql => "mysql",
            Self::Sqlite => "sqlite",
        }
    }
}

/// Feature flags the frontend uses to show or hide features. It never branches on [`DriverKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// Has a schema level under the database (PostgreSQL).
    pub schemas: bool,
    /// Can list other databases from one connection (MySQL).
    pub multiple_databases: bool,
    pub cancel: bool,
    /// Server-enforced read-only sessions.
    pub read_only_session: bool,
    pub transactional_ddl: bool,
    pub explain: bool,
    pub tls: bool,
    /// False for file databases.
    pub tunnel: bool,
}

/// How a driver reaches its database, which decides the fields on the connection form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum EndpointKind {
    Tcp,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum OptionKind {
    Bool,
    Text,
}

/// A driver-specific option shown on the connection form, stored in `ConnectionConfig::options`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OptionField {
    pub key: String,
    pub label: String,
    pub kind: OptionKind,
    pub default: Option<String>,
    pub help: Option<String>,
}

/// Static description of a driver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DriverInfo {
    pub kind: DriverKind,
    pub label: String,
    pub endpoint: EndpointKind,
    /// Pre-filled on the connection form for TCP drivers.
    pub default_port: Option<u16>,
    /// TLS modes the driver supports; empty when it has no TLS.
    pub tls_modes: Vec<TlsMode>,
    pub capabilities: Capabilities,
    pub options: Vec<OptionField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// `dial_*` is where the socket connects; `server_name` is the real host, for TLS.
    Tcp {
        dial_host: String,
        dial_port: u16,
        server_name: String,
    },
    File {
        path: PathBuf,
    },
}

/// PostgreSQL's vocabulary, used for every driver (SECURITY.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "kebab-case")]
pub enum TlsMode {
    #[default]
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

/// TLS settings. Files are PEM; they are read at connect time, never copied.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TlsConfig {
    pub mode: TlsMode,
    /// Extra CA certificate(s) trusted in addition to the platform roots.
    pub ca_file: Option<PathBuf>,
    pub client_cert_file: Option<PathBuf>,
    pub client_key_file: Option<PathBuf>,
}

impl TlsMode {
    pub const ALL: [TlsMode; 5] = [
        Self::Disable,
        Self::Prefer,
        Self::Require,
        Self::VerifyCa,
        Self::VerifyFull,
    ];

    /// Whether the server certificate chain is checked.
    pub fn verifies_certificate(self) -> bool {
        matches!(self, Self::VerifyCa | Self::VerifyFull)
    }
}

/// Everything a driver needs to connect. Built by the app from a profile and its secrets.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub endpoint: Endpoint,
    pub database: Option<String>,
    pub user: Option<String>,
    pub password: Option<SecretString>,
    pub tls: TlsConfig,
    pub read_only: bool,
    pub options: BTreeMap<String, String>,
}

impl ConnectionConfig {
    /// A configuration for a file database with no options.
    pub fn file(path: impl Into<PathBuf>, read_only: bool) -> Self {
        Self {
            endpoint: Endpoint::File { path: path.into() },
            database: None,
            user: None,
            password: None,
            tls: TlsConfig::default(),
            read_only,
            options: BTreeMap::new(),
        }
    }

    /// Reads a boolean driver option (`"true"`/`"false"`), falling back to `default`.
    pub fn bool_option(&self, key: &str, default: bool) -> bool {
        match self.options.get(key).map(String::as_str) {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        }
    }
}

#[async_trait]
pub trait Driver: Send + Sync {
    fn info(&self) -> DriverInfo;
    fn dialect(&self) -> Arc<dyn Dialect>;
    async fn connect(&self, config: &ConnectionConfig) -> Result<Arc<dyn Connection>, DbError>;
}

/// One database connection. Methods take `&self`; implementations serialise access internally.
#[async_trait]
pub trait Connection: Send + Sync {
    async fn ping(&self) -> Result<(), DbError>;

    async fn server_version(&self) -> Result<String, DbError>;

    /// Ad-hoc execution of exactly one statement.
    async fn execute(
        &self,
        sql: &str,
        options: &ExecuteOptions,
    ) -> Result<StatementOutcome, DbError>;

    fn cancel_handle(&self) -> Arc<dyn CancelHandle>;

    /// Runs one statement with bound parameters for the placeholders the dialect produced
    /// (`Dialect::placeholder`). Used for table pages and, later, edits.
    async fn query_params(
        &self,
        _sql: &str,
        _params: &[Value],
        _options: &ExecuteOptions,
    ) -> Result<StatementOutcome, DbError> {
        Err(DbError::unsupported("query_params"))
    }

    /// Runs one statement and sends its rows in batches of `STREAM_BATCH` to `rows`, without
    /// holding the result in memory (exports, task 8.3). When the receiver goes away the driver
    /// stops reading and cancels the statement (D-017). Returns the number of rows sent.
    async fn stream_rows(
        &self,
        _sql: &str,
        _params: &[Value],
        _rows: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<u64, DbError> {
        Err(DbError::unsupported("stream_rows"))
    }

    /// A cheap row count from statistics, without scanning; `None` when there is none.
    async fn estimate_rows(&self, _table: &TableRef) -> Result<Option<u64>, DbError> {
        Ok(None)
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>, DbError> {
        Err(DbError::unsupported("list_namespaces"))
    }

    /// Where unqualified names resolve (PostgreSQL's current schema, MySQL's selected database),
    /// used as autocomplete's default schema. `None` when there is none.
    async fn default_namespace(&self) -> Result<Option<NamespaceRef>, DbError> {
        Ok(None)
    }

    async fn list_objects(&self, _namespace: &NamespaceRef) -> Result<Vec<ObjectSummary>, DbError> {
        Err(DbError::unsupported("list_objects"))
    }

    async fn describe_table(&self, _table: &TableRef) -> Result<TableInfo, DbError> {
        Err(DbError::unsupported("describe_table"))
    }

    async fn table_ddl(&self, _table: &TableRef) -> Result<String, DbError> {
        Err(DbError::unsupported("table_ddl"))
    }

    async fn completion_schema(
        &self,
        _namespace: &NamespaceRef,
    ) -> Result<CompletionSchema, DbError> {
        Err(DbError::unsupported("completion_schema"))
    }

    /// Closes the connection. Later calls fail with `ConnectionLost`. Closing twice is a no-op.
    async fn close(&self) -> Result<(), DbError>;
}

#[async_trait]
pub trait CancelHandle: Send + Sync {
    async fn cancel(&self) -> Result<(), DbError>;
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::error::DbErrorCode;

    #[rstest]
    #[case(Some("true"), false, true)]
    #[case(Some("false"), true, false)]
    #[case(Some("yes"), true, true)]
    #[case(None, false, false)]
    fn bool_option_falls_back_to_default(
        #[case] stored: Option<&str>,
        #[case] default: bool,
        #[case] expected: bool,
    ) {
        let mut config = ConnectionConfig::file("x.db", false);
        if let Some(stored) = stored {
            config.options.insert("flag".into(), stored.into());
        }

        assert_eq!(config.bool_option("flag", default), expected);
    }

    #[test]
    fn only_verify_modes_check_certificates() {
        let verifying: Vec<TlsMode> = [
            TlsMode::Disable,
            TlsMode::Prefer,
            TlsMode::Require,
            TlsMode::VerifyCa,
            TlsMode::VerifyFull,
        ]
        .into_iter()
        .filter(|m| m.verifies_certificate())
        .collect();

        assert_eq!(verifying, vec![TlsMode::VerifyCa, TlsMode::VerifyFull]);
        assert_eq!(
            serde_json::to_value(TlsMode::VerifyFull).unwrap(),
            "verify-full"
        );
    }

    #[test]
    fn driver_kind_serialises_like_as_str() {
        for kind in [DriverKind::Postgres, DriverKind::Mysql, DriverKind::Sqlite] {
            let json = serde_json::to_value(kind).unwrap();
            assert_eq!(json, serde_json::Value::String(kind.as_str().into()));
        }
    }

    struct Minimal;

    struct NoCancel;

    #[async_trait]
    impl CancelHandle for NoCancel {
        async fn cancel(&self) -> Result<(), DbError> {
            Ok(())
        }
    }

    #[async_trait]
    impl Connection for Minimal {
        async fn ping(&self) -> Result<(), DbError> {
            Ok(())
        }
        async fn server_version(&self) -> Result<String, DbError> {
            Ok("1".into())
        }
        async fn execute(&self, _: &str, _: &ExecuteOptions) -> Result<StatementOutcome, DbError> {
            Ok(StatementOutcome::Affected { rows: 0 })
        }
        fn cancel_handle(&self) -> Arc<dyn CancelHandle> {
            Arc::new(NoCancel)
        }
        async fn close(&self) -> Result<(), DbError> {
            Ok(())
        }
    }

    #[test]
    fn introspection_defaults_to_unsupported() {
        let table = TableRef {
            namespace: NamespaceRef::default(),
            name: "t".into(),
        };
        let namespace = NamespaceRef::default();

        let codes = [
            block_on(Minimal.list_namespaces()).err().map(|e| e.code),
            block_on(Minimal.list_objects(&namespace))
                .err()
                .map(|e| e.code),
            block_on(Minimal.describe_table(&table))
                .err()
                .map(|e| e.code),
            block_on(Minimal.table_ddl(&table)).err().map(|e| e.code),
            block_on(Minimal.completion_schema(&namespace))
                .err()
                .map(|e| e.code),
        ];

        assert!(codes.iter().all(|c| *c == Some(DbErrorCode::Unsupported)));
    }

    #[test]
    fn default_namespace_defaults_to_none() {
        assert_eq!(block_on(Minimal.default_namespace()), Ok(None));
    }

    /// Polls a future that never actually waits (the defaults are immediately ready).
    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        loop {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
                return value;
            }
        }
    }
}
