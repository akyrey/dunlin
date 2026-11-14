//! Saved connection profiles. Secrets are not part of a profile.

use std::collections::BTreeMap;

use dbc_core::driver::DriverKind;
use serde::{Deserialize, Serialize};

use crate::{Store, StoreError};

/// A saved connection as shown to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub driver: DriverKind,
    /// `#rrggbb`.
    pub color: Option<String>,
    pub read_only: bool,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub user: Option<String>,
    pub file_path: Option<String>,
    pub options: BTreeMap<String, String>,
}

/// A profile to save: without `id` it is created, with `id` it replaces that profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDraft {
    pub id: Option<String>,
    pub name: String,
    pub driver: DriverKind,
    pub color: Option<String>,
    #[serde(default)]
    pub read_only: bool,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub user: Option<String>,
    pub file_path: Option<String>,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
}

const MAX_NAME_LENGTH: usize = 200;
const COLUMNS: &str =
    "id, name, driver, color, read_only, host, port, database, username, file_path, options";

impl Store {
    pub fn list_connections(&self) -> Result<Vec<ConnectionProfile>, StoreError> {
        let sql = format!("SELECT {COLUMNS} FROM connections ORDER BY name COLLATE NOCASE, id");
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map([], read_row)?;
        rows.map(|row| row?.into_profile()).collect()
    }

    pub fn get_connection(&self, id: &str) -> Result<ConnectionProfile, StoreError> {
        let sql = format!("SELECT {COLUMNS} FROM connections WHERE id = ?1");
        let mut statement = self.connection.prepare(&sql)?;
        let mut rows = statement.query_map([id], read_row)?;
        match rows.next() {
            Some(row) => row?.into_profile(),
            None => Err(StoreError::NotFound(id.to_owned())),
        }
    }

    pub fn save_connection(&self, draft: ConnectionDraft) -> Result<ConnectionProfile, StoreError> {
        let draft = validate(draft)?;
        let now = now_millis();
        let options = serde_json::to_string(&draft.options)?;
        let id = match &draft.id {
            Some(id) => {
                let changed = self.connection.execute(
                    "UPDATE connections SET name = ?2, driver = ?3, color = ?4, read_only = ?5,
                       host = ?6, port = ?7, database = ?8, username = ?9, file_path = ?10,
                       options = ?11, updated_at = ?12
                     WHERE id = ?1",
                    rusqlite::params![
                        id,
                        draft.name,
                        draft.driver.as_str(),
                        draft.color,
                        draft.read_only,
                        draft.host,
                        draft.port,
                        draft.database,
                        draft.user,
                        draft.file_path,
                        options,
                        now
                    ],
                )?;
                if changed == 0 {
                    return Err(StoreError::NotFound(id.clone()));
                }
                id.clone()
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                self.connection.execute(
                    "INSERT INTO connections (id, name, driver, color, read_only, host, port,
                       database, username, file_path, options, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
                    rusqlite::params![
                        id,
                        draft.name,
                        draft.driver.as_str(),
                        draft.color,
                        draft.read_only,
                        draft.host,
                        draft.port,
                        draft.database,
                        draft.user,
                        draft.file_path,
                        options,
                        now
                    ],
                )?;
                id
            }
        };
        self.get_connection(&id)
    }

    pub fn delete_connection(&self, id: &str) -> Result<(), StoreError> {
        let deleted = self
            .connection
            .execute("DELETE FROM connections WHERE id = ?1", [id])?;
        if deleted == 0 {
            return Err(StoreError::NotFound(id.to_owned()));
        }
        Ok(())
    }
}

fn validate(draft: ConnectionDraft) -> Result<ConnectionDraft, StoreError> {
    let name = draft.name.trim().to_owned();
    if name.is_empty() {
        return Err(StoreError::Invalid(
            "The connection name is required".to_owned(),
        ));
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(StoreError::Invalid(format!(
            "The connection name is longer than {MAX_NAME_LENGTH} characters"
        )));
    }
    if let Some(color) = &draft.color
        && !is_hex_color(color)
    {
        return Err(StoreError::Invalid(
            "The colour must look like #rrggbb".to_owned(),
        ));
    }
    if let Some(id) = &draft.id
        && uuid::Uuid::parse_str(id).is_err()
    {
        return Err(StoreError::Invalid(
            "The connection id is not a UUID".to_owned(),
        ));
    }
    Ok(ConnectionDraft { name, ..draft })
}

fn is_hex_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// A row as stored, before parsing the driver name and options.
struct StoredRow {
    id: String,
    name: String,
    driver: String,
    color: Option<String>,
    read_only: bool,
    host: Option<String>,
    port: Option<u16>,
    database: Option<String>,
    user: Option<String>,
    file_path: Option<String>,
    options: String,
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRow> {
    Ok(StoredRow {
        id: row.get(0)?,
        name: row.get(1)?,
        driver: row.get(2)?,
        color: row.get(3)?,
        read_only: row.get(4)?,
        host: row.get(5)?,
        port: row.get(6)?,
        database: row.get(7)?,
        user: row.get(8)?,
        file_path: row.get(9)?,
        options: row.get(10)?,
    })
}

impl StoredRow {
    fn into_profile(self) -> Result<ConnectionProfile, StoreError> {
        let driver = serde_json::from_value(serde_json::Value::String(self.driver))?;
        Ok(ConnectionProfile {
            id: self.id,
            name: self.name,
            driver,
            color: self.color,
            read_only: self.read_only,
            host: self.host,
            port: self.port,
            database: self.database,
            user: self.user,
            file_path: self.file_path,
            options: serde_json::from_str(&self.options)?,
        })
    }
}
