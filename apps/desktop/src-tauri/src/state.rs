//! Application state shared by all commands.

use std::path::Path;
use std::sync::{Arc, Mutex};

use dbc_store::Store;

use crate::error::AppError;
use crate::registry::DriverRegistry;
use crate::session::SessionManager;

pub struct AppState {
    store: Arc<Mutex<Store>>,
    pub registry: DriverRegistry,
    pub sessions: SessionManager,
}

impl AppState {
    pub fn open(store_path: &Path) -> Result<Self, AppError> {
        let store = Store::open(store_path)?;
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
            registry: DriverRegistry::builtin(),
            sessions: SessionManager::default(),
        })
    }

    /// Runs `f` against the store on a blocking thread (rusqlite is synchronous).
    pub async fn with_store<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&Store) -> Result<T, AppError> + Send + 'static,
    {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || {
            let guard = store
                .lock()
                .map_err(|_| AppError::internal("Local storage is unavailable"))?;
            f(&guard)
        })
        .await
        .map_err(|_| AppError::internal("Local storage task failed"))?
    }
}
