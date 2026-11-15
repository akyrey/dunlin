//! The key browser over Redis: `SCAN` for keys with their types, and each type's content read
//! in bounded steps (never `KEYS`, never a whole huge value).

use async_trait::async_trait;
use dbc_core::kv::{KeyContent, KeyName, KeySummary, KeyValueStore, ScanPage};
use dbc_core::result::{ColumnMeta, ResultSet, TypeCategory};
use dbc_core::{DbError, DbErrorCode, Value};
use redis::Value as Reply;

use crate::connection::RedisConnection;
use crate::reply::{bytes_cell, cell};

/// Strings longer than this are shown truncated; the cell viewer says so.
pub const MAX_STRING_BYTES: u64 = 1024 * 1024;
/// Elements read per `HSCAN`/`SSCAN` step.
const SCAN_STEP: usize = 500;

fn args(parts: &[&[u8]]) -> Vec<Vec<u8>> {
    parts.iter().map(|p| p.to_vec()).collect()
}

fn bytes(reply: Reply) -> Vec<u8> {
    match reply {
        Reply::BulkString(b) => b,
        Reply::SimpleString(s) => s.into_bytes(),
        Reply::Okay => b"OK".to_vec(),
        Reply::Int(n) => n.to_string().into_bytes(),
        Reply::Double(f) => f.to_string().into_bytes(),
        _ => Vec::new(),
    }
}

fn text(reply: Reply) -> String {
    String::from_utf8_lossy(&bytes(reply)).into_owned()
}

fn int(reply: &Reply) -> i64 {
    match reply {
        Reply::Int(n) => *n,
        Reply::BulkString(b) => String::from_utf8_lossy(b).parse().unwrap_or_default(),
        _ => 0,
    }
}

fn items(reply: Reply) -> Vec<Reply> {
    match reply {
        Reply::Array(items) | Reply::Set(items) => items,
        Reply::Map(entries) => entries.into_iter().flat_map(|(k, v)| [k, v]).collect(),
        _ => Vec::new(),
    }
}

/// `[cursor, [elements]]`, the reply of every `*SCAN`.
fn scan_reply(reply: Reply) -> (String, Vec<Reply>) {
    let mut parts = items(reply).into_iter();
    let cursor = parts.next().map(text).unwrap_or_else(|| "0".into());
    (cursor, parts.next().map(items).unwrap_or_default())
}

fn column(name: &str, category: TypeCategory) -> ColumnMeta {
    ColumnMeta {
        name: name.to_owned(),
        type_name: String::new(),
        category,
        nullable: Some(false),
    }
}

fn grid(columns: Vec<ColumnMeta>, rows: Vec<Vec<Value>>, length: u64) -> ResultSet {
    let truncated = u64::try_from(rows.len()).unwrap_or(u64::MAX) < length;
    ResultSet {
        columns,
        rows,
        truncated,
    }
}

impl RedisConnection {
    async fn reply(&self, parts: &[&[u8]]) -> Result<Reply, DbError> {
        self.command(args(parts)).await
    }

    /// `HSCAN`/`SSCAN` until `limit` elements or the end.
    async fn scan_collection(
        &self,
        command: &[u8],
        key: &[u8],
        limit: usize,
    ) -> Result<Vec<Reply>, DbError> {
        let mut found = Vec::new();
        let mut cursor = "0".to_owned();
        let step = SCAN_STEP.to_string();
        loop {
            let reply = self
                .reply(&[command, key, cursor.as_bytes(), b"COUNT", step.as_bytes()])
                .await?;
            let (next, elements) = scan_reply(reply);
            found.extend(elements);
            if next == "0" || found.len() >= limit {
                return Ok(found);
            }
            cursor = next;
        }
    }

    async fn content(
        &self,
        key: &[u8],
        key_type: &str,
        limit: usize,
    ) -> Result<(u64, ResultSet), DbError> {
        let count = limit.to_string();
        let last = limit.saturating_sub(1).to_string();
        let length = |reply: Reply| u64::try_from(int(&reply)).unwrap_or_default();
        Ok(match key_type {
            "string" => {
                let size = length(self.reply(&[b"STRLEN", key]).await?);
                let end = (MAX_STRING_BYTES - 1).to_string();
                let value = bytes(
                    self.reply(&[b"GETRANGE", key, b"0", end.as_bytes()])
                        .await?,
                );
                let rows = ResultSet {
                    columns: vec![column("value", TypeCategory::Text)],
                    rows: vec![vec![bytes_cell(value)]],
                    truncated: size > MAX_STRING_BYTES,
                };
                (size, rows)
            }
            "hash" => {
                let size = length(self.reply(&[b"HLEN", key]).await?);
                let flat = self
                    .scan_collection(b"HSCAN", key, limit.saturating_mul(2))
                    .await?;
                let rows = flat
                    .chunks(2)
                    .take(limit)
                    .map(|pair| pair.iter().cloned().map(cell).collect())
                    .collect();
                let columns = vec![
                    column("field", TypeCategory::Text),
                    column("value", TypeCategory::Text),
                ];
                (size, grid(columns, rows, size))
            }
            "list" => {
                let size = length(self.reply(&[b"LLEN", key]).await?);
                let values = items(self.reply(&[b"LRANGE", key, b"0", last.as_bytes()]).await?);
                let rows = values
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| vec![Value::Int(i64::try_from(i).unwrap_or(i64::MAX)), cell(v)])
                    .collect();
                let columns = vec![
                    column("index", TypeCategory::Integer),
                    column("value", TypeCategory::Text),
                ];
                (size, grid(columns, rows, size))
            }
            "set" => {
                let size = length(self.reply(&[b"SCARD", key]).await?);
                let members = self.scan_collection(b"SSCAN", key, limit).await?;
                let rows = members
                    .into_iter()
                    .take(limit)
                    .map(|m| vec![cell(m)])
                    .collect();
                (
                    size,
                    grid(vec![column("member", TypeCategory::Text)], rows, size),
                )
            }
            "zset" => {
                let size = length(self.reply(&[b"ZCARD", key]).await?);
                let reply = self
                    .reply(&[b"ZRANGE", key, b"0", last.as_bytes(), b"WITHSCORES"])
                    .await?;
                // RESP3 pairs each member with its score; RESP2 alternates them.
                let flat: Vec<Reply> = items(reply)
                    .into_iter()
                    .flat_map(|item| match item {
                        Reply::Array(pair) => pair,
                        other => vec![other],
                    })
                    .collect();
                let rows = flat
                    .chunks(2)
                    .map(|pair| {
                        let member = pair.first().cloned().map_or(Value::Null, cell);
                        let score = pair.get(1).map_or(Value::Null, |s| match s {
                            Reply::Double(f) => Value::Float(*f),
                            other => text(other.clone())
                                .parse()
                                .map_or(Value::Null, Value::Float),
                        });
                        vec![member, score]
                    })
                    .collect();
                let columns = vec![
                    column("member", TypeCategory::Text),
                    column("score", TypeCategory::Float),
                ];
                (size, grid(columns, rows, size))
            }
            "stream" => {
                let size = length(self.reply(&[b"XLEN", key]).await?);
                let entries = items(
                    self.reply(&[b"XRANGE", key, b"-", b"+", b"COUNT", count.as_bytes()])
                        .await?,
                );
                let rows = entries
                    .into_iter()
                    .map(|entry| {
                        let mut parts = items(entry).into_iter();
                        let id = parts.next().map_or(Value::Null, |id| Value::Text(text(id)));
                        let fields = parts.next().map_or(Value::Null, cell);
                        vec![id, fields]
                    })
                    .collect();
                let columns = vec![
                    column("id", TypeCategory::Text),
                    column("fields", TypeCategory::Json),
                ];
                (size, grid(columns, rows, size))
            }
            "ReJSON-RL" => {
                let document = bytes(self.reply(&[b"JSON.GET", key]).await?);
                let rows = ResultSet {
                    columns: vec![column("value", TypeCategory::Json)],
                    rows: vec![vec![Value::Json(
                        String::from_utf8_lossy(&document).into_owned(),
                    )]],
                    truncated: false,
                };
                (1, rows)
            }
            _ => (
                0,
                ResultSet {
                    columns: Vec::new(),
                    rows: Vec::new(),
                    truncated: false,
                },
            ),
        })
    }
}

#[async_trait]
impl KeyValueStore for RedisConnection {
    async fn scan_keys(
        &self,
        pattern: &str,
        cursor: Option<&str>,
        count: usize,
    ) -> Result<ScanPage, DbError> {
        let count = count.clamp(1, 10_000).to_string();
        let reply = self
            .reply(&[
                b"SCAN",
                cursor.unwrap_or("0").as_bytes(),
                b"MATCH",
                pattern.as_bytes(),
                b"COUNT",
                count.as_bytes(),
            ])
            .await?;
        let (next, keys) = scan_reply(reply);
        let raw: Vec<Vec<u8>> = keys.into_iter().map(bytes).collect();
        let types = if raw.is_empty() {
            Vec::new()
        } else {
            self.pipeline(raw.iter().map(|key| args(&[b"TYPE", key])).collect())
                .await?
        };
        let summaries = raw
            .iter()
            .zip(types)
            .map(|(key, key_type)| (key, text(key_type)))
            // A key deleted between SCAN and TYPE is simply left out.
            .filter(|(_, key_type)| key_type != "none")
            .map(|(key, key_type)| KeySummary {
                key: KeyName::from_bytes(key),
                key_type,
            })
            .collect();
        Ok(ScanPage {
            keys: summaries,
            cursor: (next != "0").then_some(next),
        })
    }

    async fn read_key(&self, key: &[u8], limit: usize) -> Result<KeyContent, DbError> {
        let key_type = text(self.reply(&[b"TYPE", key]).await?);
        if key_type == "none" {
            return Err(DbError::new(
                DbErrorCode::NotFound,
                "The key no longer exists",
            ));
        }
        let ttl = int(&self.reply(&[b"PTTL", key]).await?);
        let (length, rows) = self.content(key, &key_type, limit.max(1)).await?;
        Ok(KeyContent {
            key: KeyName::from_bytes(key),
            key_type,
            ttl_ms: (ttl >= 0).then_some(ttl),
            length,
            rows,
        })
    }
}
