//! Driver and connection traits, and connection configuration.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use crate::dialect::Dialect;
use crate::error::DbError;
use crate::result::{ExecuteOptions, StatementOutcome};
use crate::schema::{
    CompletionSchema, Namespace, NamespaceRef, ObjectSummary, TableInfo, TableRef,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TlsMode {
    #[default]
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

/// TLS settings. CA and client certificates arrive with the network drivers in Phase 2.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TlsConfig {
    pub mode: TlsMode,
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

    async fn list_namespaces(&self) -> Result<Vec<Namespace>, DbError> {
        Err(DbError::unsupported("list_namespaces"))
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
