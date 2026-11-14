//! SSH host keys the user trusted in the app (SECURITY.md#ssh).

use serde::{Deserialize, Serialize};

use crate::connections::now_millis;
use crate::{Store, StoreError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct KnownHost {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    /// `SHA256:...`.
    pub fingerprint: String,
}

impl Store {
    pub fn list_known_hosts(&self) -> Result<Vec<KnownHost>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT host, port, algorithm, fingerprint FROM known_hosts ORDER BY host, port, algorithm",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(KnownHost {
                host: row.get(0)?,
                port: row.get(1)?,
                algorithm: row.get(2)?,
                fingerprint: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Trusts a key. A *different* key already trusted for the same host, port and algorithm
    /// is never replaced here: the user must remove it deliberately first.
    pub fn trust_host(&self, key: &KnownHost) -> Result<(), StoreError> {
        let host = key.host.to_lowercase();
        let existing: Option<String> = self
            .connection
            .query_row(
                "SELECT fingerprint FROM known_hosts WHERE host = ?1 AND port = ?2 AND algorithm = ?3",
                rusqlite::params![host, key.port, key.algorithm],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|e| if e == rusqlite::Error::QueryReturnedNoRows { Ok(None) } else { Err(e) })?;
        match existing {
            Some(fingerprint) if fingerprint == key.fingerprint => Ok(()),
            Some(_) => Err(StoreError::Invalid(format!(
                "A different key is already trusted for {host}:{}; remove it in Settings first",
                key.port
            ))),
            None => {
                self.connection.execute(
                    "INSERT INTO known_hosts (host, port, algorithm, fingerprint, added_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![host, key.port, key.algorithm, key.fingerprint, now_millis()],
                )?;
                Ok(())
            }
        }
    }

    pub fn remove_known_host(
        &self,
        host: &str,
        port: u16,
        algorithm: &str,
    ) -> Result<(), StoreError> {
        let removed = self.connection.execute(
            "DELETE FROM known_hosts WHERE host = ?1 AND port = ?2 AND algorithm = ?3",
            rusqlite::params![host.to_lowercase(), port, algorithm],
        )?;
        if removed == 0 {
            return Err(StoreError::NotFound(format!("{host}:{port}")));
        }
        Ok(())
    }
}
