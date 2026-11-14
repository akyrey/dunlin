//! Saved connection profiles. Secrets are not part of a profile.

use std::collections::BTreeMap;

use dbc_core::driver::{DriverKind, TlsMode};
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
    pub tls: TlsSettings,
    /// A database password is stored in the keychain. The value itself is never returned.
    pub has_db_password: bool,
}

/// TLS settings as saved; files are referenced by path, never copied.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TlsSettings {
    pub mode: TlsMode,
    pub ca_file: Option<String>,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
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
    #[serde(default)]
    pub tls: TlsSettings,
}

impl From<ConnectionProfile> for ConnectionDraft {
    fn from(profile: ConnectionProfile) -> Self {
        Self {
            id: Some(profile.id),
            name: profile.name,
            driver: profile.driver,
            color: profile.color,
            read_only: profile.read_only,
            host: profile.host,
            port: profile.port,
            database: profile.database,
            user: profile.user,
            file_path: profile.file_path,
            options: profile.options,
            tls: profile.tls,
        }
    }
}

const MAX_NAME_LENGTH: usize = 200;
const COLUMNS: &str = "id, name, driver, color, read_only, host, port, database, username, \
    file_path, options, tls_mode, tls_ca_file, tls_cert_file, tls_key_file, has_db_password";

/// Both statements take the same 16 parameters, in `save_connection`'s order.
const INSERT_SQL: &str = "INSERT INTO connections (id, name, driver, color, read_only, host, port,
       database, username, file_path, options, created_at, updated_at,
       tls_mode, tls_ca_file, tls_cert_file, tls_key_file)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13, ?14, ?15, ?16)";
const UPDATE_SQL: &str = "UPDATE connections SET name = ?2, driver = ?3, color = ?4,
       read_only = ?5, host = ?6, port = ?7, database = ?8, username = ?9, file_path = ?10,
       options = ?11, updated_at = ?12, tls_mode = ?13, tls_ca_file = ?14, tls_cert_file = ?15,
       tls_key_file = ?16
     WHERE id = ?1";

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
        let tls_mode = enum_text(draft.tls.mode)?;
        let (id, sql) = match &draft.id {
            Some(id) => (id.clone(), UPDATE_SQL),
            None => (uuid::Uuid::new_v4().to_string(), INSERT_SQL),
        };
        let changed = self.connection.execute(
            sql,
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
                now,
                tls_mode,
                draft.tls.ca_file,
                draft.tls.cert_file,
                draft.tls.key_file
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id));
        }
        self.get_connection(&id)
    }

    /// Records whether the keychain holds a database password for `id`.
    pub fn set_has_db_password(&self, id: &str, saved: bool) -> Result<(), StoreError> {
        let changed = self.connection.execute(
            "UPDATE connections SET has_db_password = ?2 WHERE id = ?1",
            rusqlite::params![id, saved],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.to_owned()));
        }
        Ok(())
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

/// The serde name of a unit enum variant, e.g. `verify-full`.
fn enum_text<T: Serialize>(value: T) -> Result<String, StoreError> {
    match serde_json::to_value(value)? {
        serde_json::Value::String(text) => Ok(text),
        other => Err(StoreError::Invalid(format!(
            "unexpected enum value {other}"
        ))),
    }
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
    tls_mode: String,
    tls: TlsSettings,
    has_db_password: bool,
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
        tls_mode: row.get(11)?,
        tls: TlsSettings {
            mode: TlsMode::Disable,
            ca_file: row.get(12)?,
            cert_file: row.get(13)?,
            key_file: row.get(14)?,
        },
        has_db_password: row.get(15)?,
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
            tls: TlsSettings {
                mode: serde_json::from_value(serde_json::Value::String(self.tls_mode))?,
                ..self.tls
            },
            has_db_password: self.has_db_password,
        })
    }
}
