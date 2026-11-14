//! Local application state in one SQLite file. Secrets never go here (see SECURITY.md).

mod connections;
mod migrations;
mod secrets;

use std::path::Path;

pub use connections::{ConnectionDraft, ConnectionProfile, TlsSettings};
pub use secrets::{
    KEYCHAIN_SERVICE, KeychainSecretStore, MemorySecretStore, SecretKind, SecretStore,
};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("store database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("stored options are not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("no connection with id {0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    /// Never contains secret material.
    #[error("{0}")]
    Keychain(String),
    #[error(
        "the store was written by a newer version (schema {found}, this version supports {supported})"
    )]
    TooNew { found: u32, supported: u32 },
}

/// The application's store. Synchronous: callers run it on a blocking thread.
pub struct Store {
    connection: rusqlite::Connection,
}

impl Store {
    /// Opens the store at `path`, creating it if needed, and applies pending migrations.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let connection = rusqlite::Connection::open(path)?;
        Self::init(connection)
    }

    fn init(connection: rusqlite::Connection) -> Result<Self, StoreError> {
        connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        migrations::migrate(&connection)?;
        Ok(Self { connection })
    }

    /// Schema version after migrations.
    pub fn schema_version(&self) -> Result<u32, StoreError> {
        migrations::current_version(&self.connection)
    }
}

#[cfg(test)]
mod tests;
