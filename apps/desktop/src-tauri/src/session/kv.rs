//! The key browser runs on the session's metadata connection, like the schema browser.

use dbc_core::DbError;
use dbc_core::kv::{KeyContent, ScanPage};

use super::Session;

fn not_key_value() -> DbError {
    DbError::unsupported("key browsing (this is not a key-value connection)")
}

impl Session {
    pub async fn kv_scan(
        &self,
        pattern: &str,
        cursor: Option<&str>,
        count: usize,
    ) -> Result<ScanPage, DbError> {
        let connection = self.metadata().await?;
        let result = match connection.key_value() {
            Some(store) => store.scan_keys(pattern, cursor, count).await,
            None => Err(not_key_value()),
        };
        self.check_metadata(&connection, result).await
    }

    pub async fn kv_read(&self, key: &[u8], limit: usize) -> Result<KeyContent, DbError> {
        let connection = self.metadata().await?;
        let result = match connection.key_value() {
            Some(store) => store.read_key(key, limit).await,
            None => Err(not_key_value()),
        };
        self.check_metadata(&connection, result).await
    }
}
