//! Key-value stores (Redis, D-032): browsing keys and reading a key's content as a grid, so the
//! frontend reuses its result grid and cell viewer.

use async_trait::async_trait;
use serde::Serialize;

use crate::DbError;
use crate::result::ResultSet;

/// Keys are binary; the base64 form addresses them, the text form is for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct KeyName {
    /// UTF-8 text, with invalid bytes replaced.
    pub name: String,
    /// The exact bytes, base64 encoded.
    pub id: String,
}

impl KeyName {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        use base64::Engine as _;
        Self {
            name: String::from_utf8_lossy(bytes).into_owned(),
            id: base64::engine::general_purpose::STANDARD.encode(bytes),
        }
    }

    /// The bytes behind an `id` from the frontend.
    pub fn decode(id: &str) -> Result<Vec<u8>, DbError> {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(id)
            .map_err(|_| {
                DbError::new(
                    crate::DbErrorCode::Internal,
                    "The key id is not valid base64",
                )
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct KeySummary {
    pub key: KeyName,
    /// As the server names it: `string`, `hash`, `list`, `set`, `zset`, `stream`, `ReJSON-RL`...
    pub key_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ScanPage {
    pub keys: Vec<KeySummary>,
    /// Where the next page starts; `None` when the scan is complete.
    pub cursor: Option<String>,
}

/// A key's content as rows: a string is one cell, a hash `field`/`value`, a list `index`/
/// `value`, a set `member`, a sorted set `member`/`score`, a stream `id`/`fields`.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyContent {
    pub key: KeyName,
    pub key_type: String,
    /// Milliseconds until it expires; `None` when it does not.
    pub ttl_ms: Option<i64>,
    /// Elements (or bytes, for a string) in the whole value.
    pub length: u64,
    pub rows: ResultSet,
}

#[async_trait]
pub trait KeyValueStore: Send + Sync {
    /// One `SCAN` step from `cursor` (`None` to start) over keys matching `pattern`.
    async fn scan_keys(
        &self,
        pattern: &str,
        cursor: Option<&str>,
        count: usize,
    ) -> Result<ScanPage, DbError>;

    /// The key's content, at most `limit` elements. `NotFound` when the key does not exist.
    async fn read_key(&self, key: &[u8], limit: usize) -> Result<KeyContent, DbError>;
}
