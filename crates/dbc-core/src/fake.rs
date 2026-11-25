//! A scripted [`Connection`] for unit tests of code that runs statements: it records what it
//! is asked to run and answers `query_params` from a queue.
#![allow(clippy::unwrap_used)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::driver::{CancelHandle, Connection};
use crate::error::{DbError, DbErrorCode};
use crate::result::{ColumnMeta, ExecuteOptions, ResultSet, StatementOutcome, TypeCategory};
use crate::schema::{TableInfo, TableRef};
use crate::value::Value;

#[derive(Default)]
pub struct Scripted {
    /// Every statement, in order: `execute` and `query_params` alike.
    pub log: Mutex<Vec<String>>,
    /// The parameters of each `query_params` call.
    pub params: Mutex<Vec<Vec<Value>>>,
    /// Answers to `query_params`; an empty queue answers `Affected { rows: 0 }`.
    pub replies: Mutex<VecDeque<Result<StatementOutcome, DbError>>>,
    /// `execute` statements that fail, such as `"COMMIT"`.
    pub failing: Vec<&'static str>,
    pub estimate: Option<u64>,
    /// What `describe_table` answers; `None` is `NotFound`.
    pub described: Option<TableInfo>,
}

impl Scripted {
    pub fn answering(replies: Vec<Result<StatementOutcome, DbError>>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            ..Self::default()
        }
    }

    pub fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

/// A result with one `n` column, one row per value.
pub fn rows(values: Vec<Value>) -> Result<StatementOutcome, DbError> {
    Ok(StatementOutcome::Rows(ResultSet {
        columns: vec![ColumnMeta {
            name: "n".into(),
            type_name: String::new(),
            category: TypeCategory::Integer,
            nullable: None,
        }],
        rows: values.into_iter().map(|v| vec![v]).collect(),
        truncated: false,
    }))
}

pub fn affected(rows: u64) -> Result<StatementOutcome, DbError> {
    Ok(StatementOutcome::Affected { rows })
}

pub fn refused(code: DbErrorCode, message: &str) -> Result<StatementOutcome, DbError> {
    Err(DbError::new(code, message))
}

struct NoCancel;

#[async_trait]
impl CancelHandle for NoCancel {
    async fn cancel(&self) -> Result<(), DbError> {
        Ok(())
    }
}

#[async_trait]
impl Connection for Scripted {
    async fn ping(&self) -> Result<(), DbError> {
        Ok(())
    }
    async fn server_version(&self) -> Result<String, DbError> {
        Ok("fake".into())
    }
    async fn execute(&self, sql: &str, _: &ExecuteOptions) -> Result<StatementOutcome, DbError> {
        self.log.lock().unwrap().push(sql.to_owned());
        if self.failing.contains(&sql) {
            return Err(DbError::new(
                DbErrorCode::Execution,
                format!("{sql} failed"),
            ));
        }
        Ok(StatementOutcome::Affected { rows: 0 })
    }
    async fn query_params(
        &self,
        sql: &str,
        params: &[Value],
        _: &ExecuteOptions,
    ) -> Result<StatementOutcome, DbError> {
        self.log.lock().unwrap().push(sql.to_owned());
        self.params.lock().unwrap().push(params.to_vec());
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(StatementOutcome::Affected { rows: 0 }))
    }
    async fn estimate_rows(&self, _table: &TableRef) -> Result<Option<u64>, DbError> {
        Ok(self.estimate)
    }
    async fn describe_table(&self, table: &TableRef) -> Result<TableInfo, DbError> {
        self.described
            .clone()
            .ok_or_else(|| DbError::new(DbErrorCode::NotFound, format!("{} not found", table.name)))
    }
    fn cancel_handle(&self) -> Arc<dyn CancelHandle> {
        Arc::new(NoCancel)
    }
    async fn close(&self) -> Result<(), DbError> {
        Ok(())
    }
}

/// Runs a future that never waits on I/O (the fake answers at once) to completion.
pub fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}
