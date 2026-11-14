//! Open sessions. `SessionManager` is the only shared mutable state in the backend.

mod health;
mod schema;

pub use health::{Health, PING_INTERVAL};

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dbc_core::dialect::Dialect;
use dbc_core::driver::{Capabilities, Connection, ConnectionConfig, Driver, DriverKind};
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_core::schema::{
    CompletionSchema, Namespace, NamespaceRef, ObjectSummary, TableInfo, TableRef,
};
use dbc_core::{DbError, DbErrorCode};
use dbc_tunnel::Tunnel;
use tokio::sync::{Mutex, RwLock};

use crate::error::AppError;
use schema::SchemaCache;

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
    /// Serves the schema browser; reopened on next use after it is lost.
    metadata: Mutex<Option<Arc<dyn Connection>>>,
    schema: SchemaCache,
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
        self.on_tab(tab_id, |connection| async move {
            connection.execute(sql, options).await
        })
        .await
    }

    /// Runs `work` on the tab's own connection (an editor tab or a table data view), marked as
    /// running so it can be cancelled. A lost connection is dropped so the next call reconnects.
    pub async fn on_tab<T, F, Fut>(&self, tab_id: &str, work: F) -> Result<T, DbError>
    where
        F: FnOnce(Arc<dyn Connection>) -> Fut,
        Fut: Future<Output = Result<T, DbError>>,
    {
        let connection = self.tab_connection(tab_id).await?;
        let outcome = {
            let _running = self.mark_running(tab_id)?;
            work(Arc::clone(&connection)).await
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

    /// The metadata connection, reconnected if it was lost.
    async fn metadata(&self) -> Result<Arc<dyn Connection>, DbError> {
        let mut slot = self.metadata.lock().await;
        if let Some(connection) = slot.as_ref() {
            return Ok(Arc::clone(connection));
        }
        let connection = self.driver.connect(&self.config).await?;
        *slot = Some(Arc::clone(&connection));
        Ok(connection)
    }

    /// Drops a lost metadata connection so the next call reconnects.
    async fn check_metadata<T>(
        &self,
        connection: &Arc<dyn Connection>,
        result: Result<T, DbError>,
    ) -> Result<T, DbError> {
        if let Err(error) = &result
            && error.code == DbErrorCode::ConnectionLost
        {
            let mut slot = self.metadata.lock().await;
            if slot
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, connection))
            {
                *slot = None;
            }
            drop(slot);
            // Closing a lost connection only releases local resources.
            let _ = connection.close().await;
        }
        result
    }

    pub async fn schema_namespaces(&self) -> Result<Vec<Namespace>, DbError> {
        let connection = self.metadata().await?;
        let result = self.schema.namespaces(connection.list_namespaces()).await;
        self.check_metadata(&connection, result).await
    }

    pub async fn schema_objects(
        &self,
        namespace: &NamespaceRef,
    ) -> Result<Vec<ObjectSummary>, DbError> {
        let connection = self.metadata().await?;
        let result = self
            .schema
            .objects(namespace, connection.list_objects(namespace))
            .await;
        self.check_metadata(&connection, result).await
    }

    pub async fn schema_table(&self, table: &TableRef) -> Result<TableInfo, DbError> {
        let connection = self.metadata().await?;
        let result = self
            .schema
            .table(table, connection.describe_table(table))
            .await;
        self.check_metadata(&connection, result).await
    }

    pub async fn schema_completion(
        &self,
        namespace: &NamespaceRef,
    ) -> Result<CompletionSchema, DbError> {
        let connection = self.metadata().await?;
        let result = self
            .schema
            .completion(namespace, connection.completion_schema(namespace))
            .await;
        self.check_metadata(&connection, result).await
    }

    pub async fn schema_table_ddl(&self, table: &TableRef) -> Result<String, DbError> {
        let connection = self.metadata().await?;
        let result = self.schema.ddl(table, connection.table_ddl(table)).await;
        self.check_metadata(&connection, result).await
    }

    pub async fn default_namespace(&self) -> Result<Option<NamespaceRef>, DbError> {
        let connection = self.metadata().await?;
        let result = connection.default_namespace().await;
        self.check_metadata(&connection, result).await
    }

    /// Forgets all cached metadata: on refresh, and after DDL ran in this session.
    pub fn invalidate_schema(&self) {
        self.schema.clear();
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
        let metadata = self.metadata.lock().await.take();
        if let Some(metadata) = metadata
            && let Err(error) = metadata.close().await
        {
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
            metadata: Mutex::new(Some(metadata)),
            schema: SchemaCache::default(),
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
