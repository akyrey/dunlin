//! Open sessions. `SessionManager` is the only shared mutable state in the backend.

use std::collections::HashMap;
use std::sync::Arc;

use dbc_core::DbError;
use dbc_core::dialect::Dialect;
use dbc_core::driver::{Capabilities, Connection, ConnectionConfig, Driver, DriverKind};
use tokio::sync::{Mutex, RwLock};

use crate::error::AppError;

/// An opened connection profile: a metadata connection plus one connection per editor tab.
pub struct Session {
    pub id: String,
    pub connection_id: String,
    pub kind: DriverKind,
    pub capabilities: Capabilities,
    pub read_only: bool,
    pub dialect: Arc<dyn Dialect>,
    driver: Arc<dyn Driver>,
    config: ConnectionConfig,
    metadata: Arc<dyn Connection>,
    tabs: Mutex<HashMap<String, Arc<dyn Connection>>>,
}

impl Session {
    /// The tab's own connection, opened on first use (DECISIONS D-007).
    pub async fn tab_connection(&self, tab_id: &str) -> Result<Arc<dyn Connection>, DbError> {
        let mut tabs = self.tabs.lock().await;
        if let Some(connection) = tabs.get(tab_id) {
            return Ok(Arc::clone(connection));
        }
        let connection = self.driver.connect(&self.config).await?;
        tabs.insert(tab_id.to_owned(), Arc::clone(&connection));
        Ok(connection)
    }

    pub async fn close_tab(&self, tab_id: &str) -> Result<(), DbError> {
        let removed = self.tabs.lock().await.remove(tab_id);
        match removed {
            Some(connection) => connection.close().await,
            None => Ok(()),
        }
    }

    pub async fn open_tab_count(&self) -> usize {
        self.tabs.lock().await.len()
    }

    /// Closes every tab connection, then the metadata connection. Reports the first error.
    async fn close(&self) -> Result<(), DbError> {
        let tabs: Vec<_> = self.tabs.lock().await.drain().map(|(_, c)| c).collect();
        let mut first_error = None;
        for connection in tabs {
            if let Err(error) = connection.close().await {
                first_error.get_or_insert(error);
            }
        }
        if let Err(error) = self.metadata.close().await {
            first_error.get_or_insert(error);
        }
        first_error.map_or(Ok(()), Err)
    }
}

#[derive(Default)]
pub struct SessionManager {
    sessions: RwLock<HashMap<String, Arc<Session>>>,
}

impl SessionManager {
    /// Connects the metadata connection and registers a new session.
    pub async fn open(
        &self,
        connection_id: &str,
        driver: Arc<dyn Driver>,
        config: ConnectionConfig,
    ) -> Result<Arc<Session>, DbError> {
        let metadata = driver.connect(&config).await?;
        let info = driver.info();
        let session = Arc::new(Session {
            id: uuid::Uuid::new_v4().to_string(),
            connection_id: connection_id.to_owned(),
            kind: info.kind,
            capabilities: info.capabilities,
            read_only: config.read_only,
            dialect: driver.dialect(),
            driver,
            config,
            metadata,
            tabs: Mutex::new(HashMap::new()),
        });
        self.sessions
            .write()
            .await
            .insert(session.id.clone(), Arc::clone(&session));
        Ok(session)
    }

    pub async fn get(&self, session_id: &str) -> Result<Arc<Session>, AppError> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("The session"))
    }

    pub async fn close(&self, session_id: &str) -> Result<(), AppError> {
        let session = self
            .sessions
            .write()
            .await
            .remove(session_id)
            .ok_or_else(|| AppError::not_found("The session"))?;
        session.close().await.map_err(AppError::from)
    }

    /// Closes every session opened from `connection_id` (used when a profile is deleted).
    pub async fn close_for_connection(&self, connection_id: &str) -> Result<(), AppError> {
        let ids: Vec<String> = self
            .sessions
            .read()
            .await
            .values()
            .filter(|s| s.connection_id == connection_id)
            .map(|s| s.id.clone())
            .collect();
        let mut first_error = None;
        for id in ids {
            if let Err(error) = self.close(&id).await {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    pub async fn count(&self) -> usize {
        self.sessions.read().await.len()
    }
}
