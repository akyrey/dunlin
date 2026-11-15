#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use dbc_core::dialect::Dialect;
use dbc_core::driver::{
    CancelHandle, Connection, ConnectionConfig, Driver, DriverInfo, DriverKind, EndpointKind,
};
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_core::{DbError, DbErrorCode};

use super::super::SessionManager;
use super::*;

/// Records every statement; `lost` fails the next one as a lost connection.
#[derive(Default)]
struct Recorder {
    log: Arc<Mutex<Vec<String>>>,
    lose_next: Arc<Mutex<bool>>,
}

struct Recording {
    log: Arc<Mutex<Vec<String>>>,
    lose_next: Arc<Mutex<bool>>,
}

struct NoCancel;

#[async_trait]
impl CancelHandle for NoCancel {
    async fn cancel(&self) -> Result<(), DbError> {
        Ok(())
    }
}

#[async_trait]
impl Driver for Recorder {
    fn info(&self) -> DriverInfo {
        DriverInfo {
            kind: DriverKind::Sqlite,
            label: "Recorder".into(),
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
        Ok(Arc::new(Recording {
            log: Arc::clone(&self.log),
            lose_next: Arc::clone(&self.lose_next),
        }))
    }
}

#[async_trait]
impl Connection for Recording {
    async fn ping(&self) -> Result<(), DbError> {
        Ok(())
    }
    async fn server_version(&self) -> Result<String, DbError> {
        Ok("1".into())
    }
    async fn execute(&self, sql: &str, _: &ExecuteOptions) -> Result<StatementOutcome, DbError> {
        if std::mem::take(&mut *self.lose_next.lock().unwrap()) {
            return Err(DbError::new(DbErrorCode::ConnectionLost, "gone"));
        }
        self.log.lock().unwrap().push(sql.to_owned());
        Ok(StatementOutcome::Affected { rows: 0 })
    }
    fn cancel_handle(&self) -> Arc<dyn CancelHandle> {
        Arc::new(NoCancel)
    }
    async fn close(&self) -> Result<(), DbError> {
        Ok(())
    }
}

async fn session() -> (Arc<Session>, Arc<Mutex<Vec<String>>>, Arc<Mutex<bool>>) {
    let driver = Recorder::default();
    let (log, lose) = (Arc::clone(&driver.log), Arc::clone(&driver.lose_next));
    let manager = SessionManager::default();
    let session = manager
        .open(
            "c",
            Arc::new(driver),
            ConnectionConfig::file("/x.db", false),
            None,
        )
        .await
        .unwrap();
    (session, log, lose)
}

const OPTIONS: ExecuteOptions = ExecuteOptions { row_limit: 10 };

#[tokio::test]
async fn auto_commit_runs_statements_as_they_are() {
    let (session, log, _) = session().await;

    session
        .execute_in_transaction("t", "UPDATE a SET b = 1", &OPTIONS)
        .await
        .unwrap();

    assert_eq!(*log.lock().unwrap(), vec!["UPDATE a SET b = 1"]);
    assert!(!session.transaction("t").open);
}

#[tokio::test]
async fn manual_mode_begins_once_and_commits_on_request() {
    let (session, log, _) = session().await;
    session.set_manual("t", true).unwrap();

    session
        .execute_in_transaction("t", "UPDATE a SET b = 1", &OPTIONS)
        .await
        .unwrap();
    session
        .execute_in_transaction("t", "DELETE FROM a WHERE b = 2", &OPTIONS)
        .await
        .unwrap();
    assert!(session.transaction("t").open);
    assert!(session.set_manual("t", false).is_err());
    let after = session.finish_transaction("t", true).await.unwrap();

    assert_eq!(
        *log.lock().unwrap(),
        vec![
            "BEGIN",
            "UPDATE a SET b = 1",
            "DELETE FROM a WHERE b = 2",
            "COMMIT"
        ]
    );
    assert_eq!(
        after,
        TransactionState {
            manual: true,
            open: false
        }
    );
}

#[tokio::test]
async fn statements_the_user_writes_are_followed() {
    let (session, log, _) = session().await;

    session
        .execute_in_transaction("t", "begin", &OPTIONS)
        .await
        .unwrap();
    assert!(session.transaction("t").open);
    session
        .execute_in_transaction("t", "  ROLLBACK;", &OPTIONS)
        .await
        .unwrap();

    assert!(!session.transaction("t").open);
    assert_eq!(*log.lock().unwrap(), vec!["begin", "  ROLLBACK;"]);
}

#[tokio::test]
async fn a_lost_connection_ends_the_transaction() {
    let (session, _, lose) = session().await;
    session.set_manual("t", true).unwrap();
    session
        .execute_in_transaction("t", "UPDATE a SET b = 1", &OPTIONS)
        .await
        .unwrap();

    *lose.lock().unwrap() = true;
    let lost = session
        .execute_in_transaction("t", "SELECT 1", &OPTIONS)
        .await;

    assert_eq!(lost.unwrap_err().code, DbErrorCode::ConnectionLost);
    assert_eq!(
        session.transaction("t"),
        TransactionState {
            manual: true,
            open: false
        }
    );
}

#[tokio::test]
async fn closing_the_tab_forgets_its_mode() {
    let (session, _, _) = session().await;
    session.set_manual("t", true).unwrap();
    session
        .execute_in_transaction("t", "SELECT 1", &OPTIONS)
        .await
        .unwrap();

    session.close_tab("t").await.unwrap();

    assert_eq!(session.transaction("t"), TransactionState::default());
}
