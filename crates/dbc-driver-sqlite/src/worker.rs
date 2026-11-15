//! A SQLite connection owned by a dedicated thread and driven through a job channel.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use dbc_core::driver::{CancelHandle, Connection, ConnectionConfig, Endpoint};
use dbc_core::plan::{self, PlanNode, PlanRow};
use dbc_core::result::{
    ColumnMeta, ExecuteOptions, ResultSet, STREAM_BATCH, StatementOutcome, StreamEvent,
};
use dbc_core::schema::{
    CompletionSchema, Namespace, NamespaceRef, ObjectSummary, TableInfo, TableRef,
};
use dbc_core::{DbError, DbErrorCode, Value};
use rusqlite::{InterruptHandle, OpenFlags};
use tokio::sync::oneshot;

use crate::OPTION_CREATE_IF_MISSING;
use crate::decode::{category_for_declared, category_for_value, decode};
use crate::errors::{map_error, map_open_error};
use crate::introspect;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

type Job = Box<dyn FnOnce(&rusqlite::Connection) + Send>;

pub(crate) struct SqliteConnection {
    jobs: Mutex<Option<mpsc::Sender<Job>>>,
    finished: Mutex<Option<oneshot::Receiver<()>>>,
    interrupt: Arc<InterruptHandle>,
}

struct OpenSpec {
    path: PathBuf,
    flags: OpenFlags,
}

impl OpenSpec {
    fn from_config(config: &ConnectionConfig) -> Result<Self, DbError> {
        let Endpoint::File { path } = &config.endpoint else {
            return Err(DbError::new(
                DbErrorCode::Connect,
                "SQLite connections need a file path",
            ));
        };
        let access = if config.read_only {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        } else if config.bool_option(OPTION_CREATE_IF_MISSING, false) {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        };
        let flags = access | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI;
        Ok(Self {
            path: path.clone(),
            flags,
        })
    }
}

impl SqliteConnection {
    pub(crate) async fn open(config: &ConnectionConfig) -> Result<Self, DbError> {
        let spec = OpenSpec::from_config(config)?;
        let (ready_tx, ready_rx) = oneshot::channel();
        let (jobs_tx, jobs_rx) = mpsc::channel::<Job>();
        let (done_tx, done_rx) = oneshot::channel();
        std::thread::Builder::new()
            .name("dbc-sqlite".to_owned())
            .spawn(move || run_worker(spec, ready_tx, jobs_rx, done_tx))
            .map_err(|e| {
                DbError::new(DbErrorCode::Internal, format!("cannot start worker: {e}"))
            })?;
        let interrupt = ready_rx.await.map_err(|_| worker_gone())??;
        Ok(Self {
            jobs: Mutex::new(Some(jobs_tx)),
            finished: Mutex::new(Some(done_rx)),
            interrupt: Arc::new(interrupt),
        })
    }

    /// Runs `f` on the worker thread and waits for its result.
    async fn call<T, F>(&self, f: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: FnOnce(&rusqlite::Connection) -> Result<T, DbError> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let job: Job = Box::new(move |connection| {
            // The caller may have stopped waiting; then nobody needs the result.
            let _ = tx.send(f(connection));
        });
        {
            let guard = self.jobs.lock().map_err(|_| poisoned())?;
            let sender = guard.as_ref().ok_or_else(closed)?;
            sender.send(job).map_err(|_| closed())?;
        }
        rx.await.map_err(|_| closed())?
    }
}

fn run_worker(
    spec: OpenSpec,
    ready: oneshot::Sender<Result<InterruptHandle, DbError>>,
    jobs: mpsc::Receiver<Job>,
    done: oneshot::Sender<()>,
) {
    let connection = match open_connection(&spec) {
        Ok(connection) => connection,
        Err(error) => {
            // If `open` was abandoned there is nobody to report the error to.
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(connection.get_interrupt_handle())).is_err() {
        return;
    }
    while let Ok(job) = jobs.recv() {
        job(&connection);
    }
    drop(connection);
    // `close` may not be waiting (the connection was dropped instead).
    let _ = done.send(());
}

fn open_connection(spec: &OpenSpec) -> Result<rusqlite::Connection, DbError> {
    let connection =
        rusqlite::Connection::open_with_flags(&spec.path, spec.flags).map_err(map_open_error)?;
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(map_open_error)?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON")
        .map_err(map_open_error)?;
    // SQLite opens lazily; reading the schema proves the file is a database.
    connection
        .query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
        .map_err(map_open_error)?;
    Ok(connection)
}

/// Binds a value as SQLite stores it; booleans are integers.
fn to_sqlite(value: Value) -> Result<rusqlite::types::Value, DbError> {
    use rusqlite::types::Value as Sql;
    Ok(match value {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(i64::from(b)),
        Value::Int(i) => Sql::Integer(i),
        Value::UInt(u) => i64::try_from(u).map_or_else(|_| Sql::Text(u.to_string()), Sql::Integer),
        Value::Float(f) => Sql::Real(f),
        Value::Bytes(bytes) => Sql::Blob(bytes),
        Value::Decimal(s)
        | Value::Text(s)
        | Value::Json(s)
        | Value::Xml(s)
        | Value::Uuid(s)
        | Value::Date(s)
        | Value::Time(s)
        | Value::Timestamp(s)
        | Value::TimestampTz(s)
        | Value::Interval(s) => Sql::Text(s),
        Value::Unknown {
            text: Some(text), ..
        } => Sql::Text(text),
        Value::Array(_) | Value::Unknown { text: None, .. } => {
            return Err(DbError::new(
                DbErrorCode::Unsupported,
                "Arrays and values without a text form cannot be used as parameters yet",
            ));
        }
    })
}

/// Sends the statement's rows in batches; stops when the receiver goes away.
fn stream_statement(
    connection: &rusqlite::Connection,
    sql: &str,
    params: &[rusqlite::types::Value],
    sender: &tokio::sync::mpsc::Sender<StreamEvent>,
) -> Result<u64, DbError> {
    let mut statement = connection.prepare(sql).map_err(map_error)?;
    let declared: Vec<(String, Option<String>)> = statement
        .columns()
        .map(|c| (c.name().to_owned(), c.decl_type().map(str::to_owned)))
        .collect();
    let columns = declared
        .iter()
        .map(|(name, declared)| column_meta(name.clone(), declared.clone(), &[], 0))
        .collect();
    if sender.blocking_send(StreamEvent::Columns(columns)).is_err() {
        return Ok(0);
    }
    let mut rows = statement
        .query(rusqlite::params_from_iter(params))
        .map_err(map_error)?;
    let mut sent = 0u64;
    let mut batch = Vec::with_capacity(STREAM_BATCH);
    while let Some(row) = rows.next().map_err(map_error)? {
        let values = (0..declared.len())
            .map(|i| row.get_ref(i).map(decode).map_err(map_error))
            .collect::<Result<Vec<_>, _>>()?;
        batch.push(values);
        if batch.len() == STREAM_BATCH {
            let full = std::mem::replace(&mut batch, Vec::with_capacity(STREAM_BATCH));
            let count = full.len() as u64;
            if sender.blocking_send(StreamEvent::Rows(full)).is_err() {
                return Ok(sent);
            }
            sent += count;
        }
    }
    let count = batch.len() as u64;
    if !batch.is_empty() && sender.blocking_send(StreamEvent::Rows(batch)).is_ok() {
        sent += count;
    }
    Ok(sent)
}

fn run_statement(
    connection: &rusqlite::Connection,
    sql: &str,
    params: &[rusqlite::types::Value],
    row_limit: usize,
) -> Result<StatementOutcome, DbError> {
    let mut statement = connection.prepare(sql).map_err(map_error)?;
    if statement.column_count() == 0 {
        let before = connection.total_changes();
        statement
            .execute(rusqlite::params_from_iter(params))
            .map_err(map_error)?;
        // `changes()` keeps the count of the last DML statement, so DDL would report a stale one.
        let rows = if connection.total_changes() == before {
            0
        } else {
            connection.changes()
        };
        return Ok(StatementOutcome::Affected { rows });
    }
    let declared: Vec<(String, Option<String>)> = statement
        .columns()
        .map(|c| (c.name().to_owned(), c.decl_type().map(str::to_owned)))
        .collect();
    let (rows, truncated) = read_rows(&mut statement, params, declared.len(), row_limit)?;
    let columns = declared
        .into_iter()
        .enumerate()
        .map(|(index, (name, declared))| column_meta(name, declared, &rows, index))
        .collect();
    Ok(StatementOutcome::Rows(ResultSet {
        columns,
        rows,
        truncated,
    }))
}

/// Reads up to `limit` rows; `truncated` is true when at least one more row exists.
fn read_rows(
    statement: &mut rusqlite::Statement<'_>,
    params: &[rusqlite::types::Value],
    width: usize,
    limit: usize,
) -> Result<(Vec<Vec<Value>>, bool), DbError> {
    let mut rows = statement
        .query(rusqlite::params_from_iter(params))
        .map_err(map_error)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().map_err(map_error)? {
        if result.len() == limit {
            return Ok((result, true));
        }
        let values = (0..width)
            .map(|i| row.get_ref(i).map(decode).map_err(map_error))
            .collect::<Result<Vec<_>, _>>()?;
        result.push(values);
    }
    Ok((result, false))
}

fn column_meta(
    name: String,
    declared: Option<String>,
    rows: &[Vec<Value>],
    index: usize,
) -> ColumnMeta {
    let category = match &declared {
        Some(declared) => category_for_declared(declared),
        None => category_for_value(rows.first().and_then(|row| row.get(index))),
    };
    ColumnMeta {
        name,
        type_name: declared.unwrap_or_default(),
        category,
        nullable: None,
    }
}

#[async_trait]
impl Connection for SqliteConnection {
    async fn ping(&self) -> Result<(), DbError> {
        self.call(|c| c.query_row("SELECT 1", [], |_| Ok(())).map_err(map_error))
            .await
    }

    async fn server_version(&self) -> Result<String, DbError> {
        self.call(|_| Ok(format!("SQLite {}", rusqlite::version())))
            .await
    }

    async fn execute(
        &self,
        sql: &str,
        options: &ExecuteOptions,
    ) -> Result<StatementOutcome, DbError> {
        let sql = sql.to_owned();
        let row_limit = options.row_limit;
        self.call(move |c| run_statement(c, &sql, &[], row_limit))
            .await
    }

    async fn stream_rows(
        &self,
        sql: &str,
        params: &[Value],
        rows: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<u64, DbError> {
        let sql = sql.to_owned();
        let params = params
            .iter()
            .cloned()
            .map(to_sqlite)
            .collect::<Result<Vec<_>, _>>()?;
        self.call(move |c| stream_statement(c, &sql, &params, &rows))
            .await
    }

    async fn query_params(
        &self,
        sql: &str,
        params: &[Value],
        options: &ExecuteOptions,
    ) -> Result<StatementOutcome, DbError> {
        let sql = sql.to_owned();
        let params = params
            .iter()
            .cloned()
            .map(to_sqlite)
            .collect::<Result<Vec<_>, _>>()?;
        let row_limit = options.row_limit;
        self.call(move |c| run_statement(c, &sql, &params, row_limit))
            .await
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>, DbError> {
        self.call(introspect::list_namespaces).await
    }

    async fn list_objects(&self, namespace: &NamespaceRef) -> Result<Vec<ObjectSummary>, DbError> {
        let namespace = namespace.clone();
        self.call(move |c| introspect::list_objects(c, &namespace))
            .await
    }

    async fn describe_table(&self, table: &TableRef) -> Result<TableInfo, DbError> {
        let table = table.clone();
        self.call(move |c| introspect::describe_table(c, &table))
            .await
    }

    async fn default_namespace(&self) -> Result<Option<NamespaceRef>, DbError> {
        Ok(Some(NamespaceRef {
            catalog: None,
            schema: Some("main".to_owned()),
        }))
    }

    async fn completion_schema(
        &self,
        namespace: &NamespaceRef,
    ) -> Result<CompletionSchema, DbError> {
        let namespace = namespace.clone();
        self.call(move |c| crate::completion::completion_schema(c, &namespace))
            .await
    }

    async fn table_ddl(&self, table: &TableRef) -> Result<String, DbError> {
        let table = table.clone();
        self.call(move |c| introspect::table_ddl(c, &table)).await
    }

    /// `EXPLAIN QUERY PLAN`: rows of `(id, parent, notused, detail)`.
    async fn explain(&self, statement: &str) -> Result<PlanNode, DbError> {
        let sql = format!("EXPLAIN QUERY PLAN {}", plan::statement_body(statement));
        let outcome = self
            .execute(&sql, &ExecuteOptions { row_limit: 10_000 })
            .await?;
        let ids = plan::column_text(&outcome, 0);
        let parents = plan::column_text(&outcome, 1);
        let texts = plan::column_text(&outcome, 3);
        let rows: Vec<PlanRow> = ids
            .iter()
            .zip(&parents)
            .zip(texts)
            .map(|((id, parent), text)| PlanRow {
                id: id.parse().unwrap_or_default(),
                parent: parent.parse().unwrap_or_default(),
                text,
            })
            .collect();
        Ok(plan::from_parent_rows(&rows))
    }

    fn cancel_handle(&self) -> Arc<dyn CancelHandle> {
        Arc::new(SqliteCancel {
            interrupt: Arc::clone(&self.interrupt),
        })
    }

    async fn close(&self) -> Result<(), DbError> {
        let sender = self.jobs.lock().map_err(|_| poisoned())?.take();
        drop(sender);
        let finished = self.finished.lock().map_err(|_| poisoned())?.take();
        if let Some(finished) = finished {
            // An error only means the worker already exited, which is the goal.
            let _ = finished.await;
        }
        Ok(())
    }
}

struct SqliteCancel {
    interrupt: Arc<InterruptHandle>,
}

#[async_trait]
impl CancelHandle for SqliteCancel {
    async fn cancel(&self) -> Result<(), DbError> {
        self.interrupt.interrupt();
        Ok(())
    }
}

fn closed() -> DbError {
    DbError::new(DbErrorCode::ConnectionLost, "The connection is closed")
}

fn worker_gone() -> DbError {
    DbError::new(
        DbErrorCode::Internal,
        "The SQLite worker stopped unexpectedly",
    )
}

fn poisoned() -> DbError {
    DbError::new(
        DbErrorCode::Internal,
        "Connection state is unavailable after a panic",
    )
}
