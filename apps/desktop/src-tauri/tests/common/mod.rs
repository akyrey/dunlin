//! Test fixture shared by the command tests: an app state over a temp store, and SQLite
//! databases loaded with the shared fixture.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dbc_core::conformance::SQLITE_FIXTURE;
use dbc_core::driver::DriverKind;
use dbc_store::{ConnectionDraft, ConnectionProfile, MemorySecretStore, TlsSettings};
use dunlin_lib::commands;
use dunlin_lib::ipc::{
    ConnectionIdArgs, ConnectionSaveArgs, QueryExecuteArgs, QueryResult, SessionInfo,
};
use dunlin_lib::state::AppState;
use tempfile::TempDir;

pub struct Fixture {
    pub dir: TempDir,
    pub state: AppState,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecretStore::default());
        let state = AppState::open(&dir.path().join("store.db"), secrets).unwrap();
        Self { dir, state }
    }

    pub fn database(&self) -> PathBuf {
        let path = self.dir.path().join(format!("{}.db", uuid::Uuid::new_v4()));
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(SQLITE_FIXTURE)
            .unwrap();
        path
    }

    pub async fn saved(&self, path: &Path, read_only: bool) -> ConnectionProfile {
        let profile = ConnectionDraft {
            read_only,
            ..draft(path)
        };
        commands::connection_save(
            &self.state,
            ConnectionSaveArgs {
                profile,
                secrets: None,
            },
        )
        .await
        .unwrap()
    }

    pub async fn session(&self, read_only: bool) -> SessionInfo {
        self.session_on(&self.database(), read_only).await
    }

    pub async fn session_on(&self, path: &Path, read_only: bool) -> SessionInfo {
        let profile = self.saved(path, read_only).await;
        let args = ConnectionIdArgs {
            connection_id: profile.id,
        };
        commands::session_open(&self.state, args).await.unwrap()
    }

    pub async fn run(&self, session: &SessionInfo, sql: &str) -> Result<QueryResult, String> {
        self.run_with(session, sql, 1000, None).await
    }

    pub async fn run_with(
        &self,
        session: &SessionInfo,
        sql: &str,
        row_limit: usize,
        confirmed: Option<bool>,
    ) -> Result<QueryResult, String> {
        let args = QueryExecuteArgs {
            session_id: session.session_id.clone(),
            tab_id: "tab-1".into(),
            sql: sql.into(),
            row_limit,
            confirmed,
        };
        commands::query_execute(&self.state, args)
            .await
            .map_err(|e| e.code)
    }
}

pub fn draft(path: &Path) -> ConnectionDraft {
    ConnectionDraft {
        id: None,
        name: "fixture".into(),
        driver: DriverKind::Sqlite,
        color: None,
        read_only: false,
        host: None,
        port: None,
        database: None,
        user: None,
        file_path: Some(path.to_string_lossy().into_owned()),
        options: BTreeMap::new(),
        tls: TlsSettings::default(),
        ssh: None,
        always_preview: false,
        folder: None,
    }
}
