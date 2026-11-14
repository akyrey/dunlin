//! Saved connection profiles: persistence and validation.

use dbc_core::driver::TlsMode;
use serde::Serialize;

use crate::profile::{
    ConnectionDraft, ConnectionProfile, SshAuthMethod, SshHostSettings, TlsSettings,
};
use crate::secrets::SecretKind;
use crate::{Store, StoreError};

const MAX_NAME_LENGTH: usize = 200;
const COLUMNS: &str = "id, name, driver, color, read_only, host, port, database, username, \
    file_path, options, tls_mode, tls_ca_file, tls_cert_file, tls_key_file, ssh, saved_secrets";

/// Both statements take the same 17 parameters, in `save_connection`'s order.
const INSERT_SQL: &str = "INSERT INTO connections (id, name, driver, color, read_only, host, port,
       database, username, file_path, options, created_at, updated_at,
       tls_mode, tls_ca_file, tls_cert_file, tls_key_file, ssh)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13, ?14, ?15, ?16, ?17)";
const UPDATE_SQL: &str = "UPDATE connections SET name = ?2, driver = ?3, color = ?4,
       read_only = ?5, host = ?6, port = ?7, database = ?8, username = ?9, file_path = ?10,
       options = ?11, updated_at = ?12, tls_mode = ?13, tls_ca_file = ?14, tls_cert_file = ?15,
       tls_key_file = ?16, ssh = ?17
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
        let options = serde_json::to_string(&draft.options)?;
        let ssh = draft.ssh.as_ref().map(serde_json::to_string).transpose()?;
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
                now_millis(),
                enum_text(draft.tls.mode)?,
                draft.tls.ca_file,
                draft.tls.cert_file,
                draft.tls.key_file,
                ssh
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id));
        }
        self.get_connection(&id)
    }

    /// Records whether the keychain holds a secret of `kind` for connection `id`.
    pub fn set_secret_saved(
        &self,
        id: &str,
        kind: SecretKind,
        saved: bool,
    ) -> Result<(), StoreError> {
        let profile = self.get_connection(id)?;
        let mut kinds: Vec<SecretKind> = profile
            .saved_secrets
            .into_iter()
            .filter(|k| *k != kind)
            .collect();
        if saved {
            kinds.push(kind);
        }
        kinds.sort();
        self.connection.execute(
            "UPDATE connections SET saved_secrets = ?2 WHERE id = ?1",
            rusqlite::params![id, serde_json::to_string(&kinds)?],
        )?;
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

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(message.into())
}

fn validate(draft: ConnectionDraft) -> Result<ConnectionDraft, StoreError> {
    let name = draft.name.trim().to_owned();
    if name.is_empty() {
        return Err(invalid("The connection name is required"));
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(invalid(format!(
            "The connection name is longer than {MAX_NAME_LENGTH} characters"
        )));
    }
    if let Some(color) = &draft.color
        && !is_hex_color(color)
    {
        return Err(invalid("The colour must look like #rrggbb"));
    }
    if let Some(id) = &draft.id
        && uuid::Uuid::parse_str(id).is_err()
    {
        return Err(invalid("The connection id is not a UUID"));
    }
    if let Some(ssh) = &draft.ssh {
        validate_ssh_host(&ssh.server, "SSH")?;
        if let Some(jump) = &ssh.jump {
            validate_ssh_host(jump, "Jump host")?;
        }
    }
    Ok(ConnectionDraft { name, ..draft })
}

fn validate_ssh_host(host: &SshHostSettings, label: &str) -> Result<(), StoreError> {
    if host.host.trim().is_empty() {
        return Err(invalid(format!("{label}: the host is required")));
    }
    if host.user.trim().is_empty() {
        return Err(invalid(format!("{label}: the user is required")));
    }
    if host.port == 0 {
        return Err(invalid(format!("{label}: the port is required")));
    }
    let has_key = host
        .key_file
        .as_deref()
        .is_some_and(|k| !k.trim().is_empty());
    if host.auth == SshAuthMethod::KeyFile && !has_key {
        return Err(invalid(format!("{label}: choose a private key file")));
    }
    Ok(())
}

fn is_hex_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// The serde name of a unit enum variant, e.g. `verify-full`.
fn enum_text<T: Serialize>(value: T) -> Result<String, StoreError> {
    match serde_json::to_value(value)? {
        serde_json::Value::String(text) => Ok(text),
        other => Err(invalid(format!("unexpected enum value {other}"))),
    }
}

pub(crate) fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// A row as stored, before parsing its JSON and enum columns.
struct StoredRow {
    profile: ConnectionProfile,
    driver: String,
    options: String,
    tls_mode: String,
    ssh: Option<String>,
    saved_secrets: String,
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRow> {
    let profile = ConnectionProfile {
        id: row.get(0)?,
        name: row.get(1)?,
        driver: dbc_core::driver::DriverKind::Sqlite,
        color: row.get(3)?,
        read_only: row.get(4)?,
        host: row.get(5)?,
        port: row.get(6)?,
        database: row.get(7)?,
        user: row.get(8)?,
        file_path: row.get(9)?,
        options: Default::default(),
        tls: TlsSettings {
            mode: TlsMode::Disable,
            ca_file: row.get(12)?,
            cert_file: row.get(13)?,
            key_file: row.get(14)?,
        },
        ssh: None,
        saved_secrets: Vec::new(),
    };
    Ok(StoredRow {
        profile,
        driver: row.get(2)?,
        options: row.get(10)?,
        tls_mode: row.get(11)?,
        ssh: row.get(15)?,
        saved_secrets: row.get(16)?,
    })
}

impl StoredRow {
    fn into_profile(self) -> Result<ConnectionProfile, StoreError> {
        let text = |s: String| serde_json::Value::String(s);
        Ok(ConnectionProfile {
            driver: serde_json::from_value(text(self.driver))?,
            options: serde_json::from_str(&self.options)?,
            tls: TlsSettings {
                mode: serde_json::from_value(text(self.tls_mode))?,
                ..self.profile.tls
            },
            ssh: self.ssh.as_deref().map(serde_json::from_str).transpose()?,
            saved_secrets: serde_json::from_str(&self.saved_secrets)?,
            ..self.profile
        })
    }
}
