#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use dbc_core::DbError;
use dbc_core::DbErrorCode;
use dbc_core::dialect::Dialect;
use dbc_core::driver::{
    CancelHandle, ConnectionConfig, Driver, DriverInfo, DriverKind, EndpointKind,
};
use dbc_core::result::{ExecuteOptions, StatementOutcome};

use super::*;

/// Connections that stop answering when `down` is set; new ones fail while it is.
#[derive(Default)]
struct Flaky {
    down: Arc<AtomicBool>,
    connects: AtomicUsize,
}

struct FlakyConnection {
    down: Arc<AtomicBool>,
    /// Set for connections made before the outage: they stay dead once it is over.
    dead: AtomicBool,
}

struct NoCancel;

#[async_trait]
impl CancelHandle for NoCancel {
    async fn cancel(&self) -> Result<(), DbError> {
        Ok(())
    }
}

#[async_trait]
impl Driver for Flaky {
    fn info(&self) -> DriverInfo {
        DriverInfo {
            kind: DriverKind::Sqlite,
            label: "Flaky".into(),
            endpoint: EndpointKind::File,
            default_port: None,
            tls_modes: Vec::new(),
            capabilities: Default::default(),
            options: Vec::new(),
        }
    }
    fn dialect(&self) -> Arc<dyn Dialect> {
        Arc::new(dbc_driver_sqlite::SqliteDialect)
    }
    async fn connect(&self, _: &ConnectionConfig) -> Result<Arc<dyn Connection>, DbError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(DbError::new(DbErrorCode::Connect, "server unreachable"));
        }
        self.connects.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(FlakyConnection {
            down: Arc::clone(&self.down),
            dead: AtomicBool::new(false),
        }))
    }
}

#[async_trait]
impl Connection for FlakyConnection {
    async fn ping(&self) -> Result<(), DbError> {
        if self.down.load(Ordering::SeqCst) {
            self.dead.store(true, Ordering::SeqCst);
        }
        if self.dead.load(Ordering::SeqCst) {
            return Err(DbError::new(DbErrorCode::ConnectionLost, "gone"));
        }
        Ok(())
    }
    async fn server_version(&self) -> Result<String, DbError> {
        Ok("1".into())
    }
    async fn execute(&self, _: &str, _: &ExecuteOptions) -> Result<StatementOutcome, DbError> {
        Ok(StatementOutcome::Affected { rows: 0 })
    }
    fn cancel_handle(&self) -> Arc<dyn CancelHandle> {
        Arc::new(NoCancel)
    }
    async fn close(&self) -> Result<(), DbError> {
        Ok(())
    }
}

async fn open(driver: Arc<Flaky>) -> (SessionManager, Arc<Session>) {
    let manager = SessionManager::default();
    let session = manager
        .open(
            "c1",
            driver,
            ConnectionConfig::file("/tmp/x.db", false),
            None,
        )
        .await
        .unwrap();
    (manager, session)
}

#[tokio::test]
async fn a_healthy_session_keeps_its_connections() {
    let driver = Arc::new(Flaky::default());
    let (manager, session) = open(Arc::clone(&driver)).await;
    session.tab_connection("tab").await.unwrap();

    let report = manager.check_all().await;

    assert_eq!(report, vec![(session.id.clone(), Health::Ok)]);
    assert_eq!(session.open_tab_count().await, 1);
    assert_eq!(driver.connects.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn an_outage_is_reported_then_idle_connections_reconnect_when_it_is_over() {
    let driver = Arc::new(Flaky::default());
    let (_manager, session) = open(Arc::clone(&driver)).await;
    session.tab_connection("tab").await.unwrap();

    driver.down.store(true, Ordering::SeqCst);
    let during = session.check_health().await;
    driver.down.store(false, Ordering::SeqCst);
    let after = session.check_health().await;
    session
        .execute("tab", "SELECT 1", &ExecuteOptions::default())
        .await
        .unwrap();

    assert_eq!(during, Health::Unreachable);
    assert_eq!(after, Health::Ok);
    // The dead tab connection was dropped; the statement opened a new one.
    assert_eq!(driver.connects.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn a_tab_with_a_statement_in_flight_is_not_pinged() {
    let driver = Arc::new(Flaky::default());
    let (_manager, session) = open(Arc::clone(&driver)).await;
    let connection = session.tab_connection("busy").await.unwrap();
    let _running = session.mark_running("busy").unwrap();

    driver.down.store(true, Ordering::SeqCst);
    session.check_health().await;
    driver.down.store(false, Ordering::SeqCst);

    assert_eq!(session.open_tab_count().await, 1);
    assert!(Arc::ptr_eq(
        &session.tab_connection("busy").await.unwrap(),
        &connection
    ));
}
