//! Application state shared by all commands.

use std::path::Path;
use std::sync::{Arc, Mutex};

use dbc_store::{SecretStore, Settings, Store};

use crate::error::AppError;
use crate::registry::DriverRegistry;
use crate::session::SessionManager;

pub struct AppState {
    store: Arc<Mutex<Store>>,
    /// Read on every statement (history cap), so kept here rather than read from the store.
    settings: std::sync::RwLock<Settings>,
    secrets: Arc<dyn SecretStore>,
    pub registry: DriverRegistry,
    pub sessions: SessionManager,
}

impl AppState {
    pub fn open(store_path: &Path, secrets: Arc<dyn SecretStore>) -> Result<Self, AppError> {
        Self::with_registry(store_path, secrets, DriverRegistry::builtin())
    }

    pub fn with_registry(
        store_path: &Path,
        secrets: Arc<dyn SecretStore>,
        registry: DriverRegistry,
    ) -> Result<Self, AppError> {
        let store = Store::open(store_path)?;
        let settings = store.settings()?;
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
            settings: std::sync::RwLock::new(settings),
            secrets,
            registry,
            sessions: SessionManager::default(),
        })
    }

    pub fn settings(&self) -> Settings {
        // A poisoned lock still holds the last settings written, which are valid.
        self.settings
            .read()
            .map_or_else(|e| e.into_inner().clone(), |s| s.clone())
    }

    pub(crate) fn replace_settings(&self, settings: Settings) {
        match self.settings.write() {
            Ok(mut current) => *current = settings,
            Err(e) => *e.into_inner() = settings,
        }
    }

    /// Runs `f` against the secret store on a blocking thread (keychain calls block).
    pub async fn with_secrets<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&dyn SecretStore) -> Result<T, AppError> + Send + 'static,
    {
        let secrets = Arc::clone(&self.secrets);
        tokio::task::spawn_blocking(move || f(secrets.as_ref()))
            .await
            .map_err(|_| AppError::internal("Keychain task failed"))?
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
