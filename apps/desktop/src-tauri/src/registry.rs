//! The drivers compiled into the app.

use std::collections::BTreeMap;
use std::sync::Arc;

use dbc_core::driver::{Driver, DriverInfo, DriverKind};
use dbc_driver_mysql::MysqlDriver;
use dbc_driver_postgres::PostgresDriver;
use dbc_driver_sqlite::SqliteDriver;

use crate::error::AppError;

#[derive(Clone)]
pub struct DriverRegistry {
    drivers: BTreeMap<DriverKind, Arc<dyn Driver>>,
}

impl DriverRegistry {
    /// Every driver available in this build.
    pub fn builtin() -> Self {
        Self::with(vec![
            Arc::new(PostgresDriver),
            Arc::new(MysqlDriver),
            Arc::new(SqliteDriver),
        ])
    }

    /// A registry with exactly these drivers.
    pub fn with(drivers: Vec<Arc<dyn Driver>>) -> Self {
        Self {
            drivers: drivers.into_iter().map(|d| (d.info().kind, d)).collect(),
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
    fn builtin_registry_has_all_three_drivers() {
        let kinds: Vec<DriverKind> = DriverRegistry::builtin()
            .list()
            .iter()
            .map(|d| d.kind)
            .collect();

        assert_eq!(
            kinds,
            vec![DriverKind::Postgres, DriverKind::Mysql, DriverKind::Sqlite]
        );
    }

    #[test]
    fn missing_driver_is_unsupported() {
        let registry = DriverRegistry::with(vec![Arc::new(SqliteDriver)]);

        let error = registry.get(DriverKind::Postgres).err().unwrap();

        assert_eq!(error.code, "unsupported");
    }
}
