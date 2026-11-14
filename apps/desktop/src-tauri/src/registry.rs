//! The drivers compiled into the app.

use std::collections::BTreeMap;
use std::sync::Arc;

use dbc_core::driver::{Driver, DriverInfo, DriverKind};
use dbc_driver_sqlite::SqliteDriver;

use crate::error::AppError;

#[derive(Clone)]
pub struct DriverRegistry {
    drivers: BTreeMap<DriverKind, Arc<dyn Driver>>,
}

impl DriverRegistry {
    /// Every driver available in this build.
    pub fn builtin() -> Self {
        let sqlite: Arc<dyn Driver> = Arc::new(SqliteDriver);
        Self {
            drivers: BTreeMap::from([(DriverKind::Sqlite, sqlite)]),
        }
    }

    pub fn list(&self) -> Vec<DriverInfo> {
        self.drivers.values().map(|d| d.info()).collect()
    }

    pub fn get(&self, kind: DriverKind) -> Result<Arc<dyn Driver>, AppError> {
        self.drivers.get(&kind).cloned().ok_or_else(|| {
            AppError::new(
                "unsupported",
                format!("The {} driver is not available yet", kind.as_str()),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_has_sqlite() {
        let registry = DriverRegistry::builtin();

        assert!(registry.get(DriverKind::Sqlite).is_ok());
        assert_eq!(registry.list().len(), 1);
    }

    #[test]
    fn missing_driver_is_unsupported() {
        let error = DriverRegistry::builtin()
            .get(DriverKind::Postgres)
            .err()
            .unwrap();

        assert_eq!(error.code, "unsupported");
    }
}
