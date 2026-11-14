//! Forward-only migrations tracked with `PRAGMA user_version`.

use crate::StoreError;

/// Migration `i` upgrades the schema from version `i` to `i + 1`. Never edit a released entry.
const MIGRATIONS: &[&str] = &[
    // 1: saved connection profiles (secrets live in the keychain, not here).
    "CREATE TABLE connections (
        id         TEXT PRIMARY KEY NOT NULL,
        name       TEXT NOT NULL,
        driver     TEXT NOT NULL,
        color      TEXT,
        read_only  INTEGER NOT NULL DEFAULT 0,
        host       TEXT,
        port       INTEGER,
        database   TEXT,
        username   TEXT,
        file_path  TEXT,
        options    TEXT NOT NULL DEFAULT '{}',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );",
    // 2: TLS settings, and whether a database password is in the keychain (never the value).
    "ALTER TABLE connections ADD COLUMN tls_mode TEXT NOT NULL DEFAULT 'disable';
     ALTER TABLE connections ADD COLUMN tls_ca_file TEXT;
     ALTER TABLE connections ADD COLUMN tls_cert_file TEXT;
     ALTER TABLE connections ADD COLUMN tls_key_file TEXT;
     ALTER TABLE connections ADD COLUMN has_db_password INTEGER NOT NULL DEFAULT 0;",
    // 3: SSH tunnels. One list of saved secret kinds replaces the single password flag, and
    // the app's own trusted SSH host keys (the user's ~/.ssh/known_hosts is only read).
    "ALTER TABLE connections ADD COLUMN ssh TEXT;
     ALTER TABLE connections ADD COLUMN saved_secrets TEXT NOT NULL DEFAULT '[]';
     UPDATE connections SET saved_secrets = '[\"db_password\"]' WHERE has_db_password = 1;
     ALTER TABLE connections DROP COLUMN has_db_password;
     CREATE TABLE known_hosts (
        host        TEXT NOT NULL,
        port        INTEGER NOT NULL,
        algorithm   TEXT NOT NULL,
        fingerprint TEXT NOT NULL,
        added_at    INTEGER NOT NULL,
        PRIMARY KEY (host, port, algorithm)
     );",
    // 4: query history, per connection and capped by the app (ARCHITECTURE.md#store).
    "CREATE TABLE query_history (
        id            INTEGER PRIMARY KEY AUTOINCREMENT,
        connection_id TEXT NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
        statement     TEXT NOT NULL,
        started_at    INTEGER NOT NULL,
        duration_ms   INTEGER NOT NULL,
        outcome       TEXT NOT NULL,
        row_count     INTEGER,
        truncated     INTEGER NOT NULL DEFAULT 0,
        error         TEXT
     );
     CREATE INDEX query_history_by_connection ON query_history (connection_id, id);",
    // 5: always show the SQL preview before applying edits on this connection (task 7.11).
    "ALTER TABLE connections ADD COLUMN always_preview INTEGER NOT NULL DEFAULT 0;",
    // 6: saved queries, for one connection (deleted with it) or for all (task 8.6).
    "CREATE TABLE saved_queries (
        id            TEXT PRIMARY KEY NOT NULL,
        connection_id TEXT REFERENCES connections (id) ON DELETE CASCADE,
        name          TEXT NOT NULL,
        sql           TEXT NOT NULL,
        created_at    INTEGER NOT NULL,
        updated_at    INTEGER NOT NULL
     );",
    // 7: application settings, one JSON document per key (task 8.8).
    "CREATE TABLE settings (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);",
    // 8: folders and a user-chosen order (task 8.13), starting from the old name order.
    "ALTER TABLE connections ADD COLUMN folder TEXT;
     ALTER TABLE connections ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
     UPDATE connections SET position = (
       SELECT COUNT(*) FROM connections AS other
       WHERE other.name COLLATE NOCASE < connections.name COLLATE NOCASE
          OR (other.name COLLATE NOCASE = connections.name COLLATE NOCASE
              AND other.id < connections.id)
     );",
];

pub(crate) fn latest_version() -> u32 {
    // The list is tiny; this cannot overflow.
    MIGRATIONS.len() as u32
}

pub(crate) fn current_version(connection: &rusqlite::Connection) -> Result<u32, StoreError> {
    Ok(connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

/// Applies every pending migration, each in its own transaction with its version bump.
pub(crate) fn migrate(connection: &rusqlite::Connection) -> Result<(), StoreError> {
    let found = current_version(connection)?;
    let supported = latest_version();
    if found > supported {
        return Err(StoreError::TooNew { found, supported });
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(found as usize) {
        let version = index + 1;
        connection.execute_batch(&format!(
            "BEGIN; {sql}\nPRAGMA user_version = {version}; COMMIT;"
        ))?;
    }
    Ok(())
}
