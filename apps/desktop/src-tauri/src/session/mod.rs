//! Open sessions. `SessionManager` is the only shared mutable state in the backend.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dbc_core::dialect::Dialect;
use dbc_core::driver::{Capabilities, Connection, ConnectionConfig, Driver, DriverKind};
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_core::{DbError, DbErrorCode};
use dbc_tunnel::Tunnel;
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
    /// Tabs with a statement in flight; only these can be cancelled.
    running: std::sync::Mutex<HashSet<String>>,
    /// The SSH tunnel every connection of this session dials through, if any.
    tunnel: std::sync::Mutex<Option<Tunnel>>,
}

/// Removes a tab from the running set when the statement finishes, however it finishes.
struct Running<'a> {
    session: &'a Session,
    tab_id: String,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        if let Ok(mut running) = self.session.running.lock() {
            running.remove(&self.tab_id);
        }
    }
}

impl Session {
    /// Runs one statement on the tab's connection. A connection found to be lost is dropped,
    /// so the next run reconnects; the statement itself is never re-run (ARCHITECTURE.md).
    pub async fn execute(
        &self,
        tab_id: &str,
        sql: &str,
        options: &ExecuteOptions,
    ) -> Result<StatementOutcome, DbError> {
        let connection = self.tab_connection(tab_id).await?;
        let outcome = {
            let _running = self.mark_running(tab_id)?;
            connection.execute(sql, options).await
        };
        if let Err(error) = &outcome
            && error.code == DbErrorCode::ConnectionLost
        {
            self.forget_tab(tab_id, &connection).await;
        }
        outcome
    }

    fn mark_running(&self, tab_id: &str) -> Result<Running<'_>, DbError> {
        let mut running = self.running.lock().map_err(|_| poisoned())?;
        if !running.insert(tab_id.to_owned()) {
            return Err(DbError::new(
                DbErrorCode::Internal,
                "A statement is already running in this tab",
            ));
        }
        Ok(Running {
            session: self,
            tab_id: tab_id.to_owned(),
        })
    }

    /// Cancels the tab's running statement. Without one, this does nothing.
    pub async fn cancel(&self, tab_id: &str) -> Result<(), DbError> {
        let is_running = self
            .running
            .lock()
            .map_err(|_| poisoned())?
            .contains(tab_id);
        if !is_running {
            return Ok(());
        }
        let connection = self.tabs.lock().await.get(tab_id).cloned();
        match connection {
            Some(connection) => connection.cancel_handle().cancel().await,
            None => Ok(()),
        }
    }

    /// Drops a lost connection, unless the tab already has a newer one.
    async fn forget_tab(&self, tab_id: &str, lost: &Arc<dyn Connection>) {
        let mut tabs = self.tabs.lock().await;
        if tabs
            .get(tab_id)
            .is_some_and(|current| Arc::ptr_eq(current, lost))
        {
            tabs.remove(tab_id);
        }
        drop(tabs);
        // Closing a lost connection only releases local resources; its error carries nothing.
        let _ = lost.close().await;
    }

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

    /// The tunnel's loopback address, while the session has one.
    pub fn tunnel_addr(&self) -> Option<std::net::SocketAddr> {
        self.tunnel
            .lock()
            .ok()
            .and_then(|tunnel| tunnel.as_ref().map(Tunnel::local_addr))
    }

    /// The SSH connection under this session has dropped (task 3.8).
    pub fn tunnel_lost(&self) -> bool {
        self.tunnel
            .lock()
            .is_ok_and(|tunnel| tunnel.as_ref().is_some_and(Tunnel::is_lost))
    }

    /// Closes every tab connection, the metadata connection, then the tunnel.
    /// Reports the first error.
    async fn close(&self) -> Result<(), DbError> {
        let result = self.close_connections().await;
        let tunnel = self.tunnel.lock().ok().and_then(|mut slot| slot.take());
        crate::connect::discard(tunnel).await;
        result
    }

    async fn close_connections(&self) -> Result<(), DbError> {
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
    /// Connects the metadata connection (through `tunnel`, if given) and registers a new
    /// session. On failure the tunnel is closed.
    pub async fn open(
        &self,
        connection_id: &str,
        driver: Arc<dyn Driver>,
        config: ConnectionConfig,
        tunnel: Option<Tunnel>,
    ) -> Result<Arc<Session>, DbError> {
        let metadata = match driver.connect(&config).await {
            Ok(metadata) => metadata,
            Err(error) => {
                crate::connect::discard(tunnel).await;
                return Err(error);
            }
        };
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
            running: std::sync::Mutex::new(HashSet::new()),
            tunnel: std::sync::Mutex::new(tunnel),
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

fn poisoned() -> DbError {
    DbError::new(
        DbErrorCode::Internal,
        "Session state is unavailable after a panic",
    )
}
