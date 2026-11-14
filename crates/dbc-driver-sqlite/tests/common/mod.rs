//! Test helpers: temp databases loaded with the shared fixture.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::PathBuf;
use std::sync::Mutex;

use dbc_core::conformance::SQLITE_FIXTURE;
use dbc_core::driver::ConnectionConfig;
use dbc_driver_sqlite::OPTION_CREATE_IF_MISSING;
use tempfile::TempDir;

/// Owns temp directories so databases outlive the connections that use them.
#[derive(Default)]
pub struct TempDatabases {
    dirs: Mutex<Vec<TempDir>>,
}

impl TempDatabases {
    pub fn dir(&self) -> PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        self.dirs.lock().unwrap().push(dir);
        path
    }

    /// A new database file with the fixture loaded.
    pub fn fixture(&self) -> PathBuf {
        let path = self.dir().join("fixture.db");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch(SQLITE_FIXTURE).unwrap();
        path
    }
}

pub fn config(path: impl Into<PathBuf>, read_only: bool) -> ConnectionConfig {
    ConnectionConfig::file(path, read_only)
}

pub fn creating(path: impl Into<PathBuf>) -> ConnectionConfig {
    let mut config = ConnectionConfig::file(path, false);
    config
        .options
        .insert(OPTION_CREATE_IF_MISSING.to_owned(), "true".to_owned());
    config
}
