//! A MySQL/MariaDB connection over the text protocol.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use dbc_core::driver::{CancelHandle, Connection, ConnectionConfig, Endpoint, TlsConfig, TlsMode};
use dbc_core::result::{ExecuteOptions, ResultSet, STREAM_BATCH, StatementOutcome, StreamEvent};
use dbc_core::schema::{
    CompletionSchema, Namespace, NamespaceRef, ObjectSummary, TableInfo, TableRef,
};
use dbc_core::{DbError, DbErrorCode, Value, sql};
use mysql_async::prelude::{Protocol, Queryable};
use mysql_async::{
    ClientIdentity, Conn, DriverError, Error, Opts, OptsBuilder, Params, QueryResult, SslOpts,
    Value as MyValue,
};
use secrecy::ExposeSecret;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use crate::decode::{column_meta, decode};
use crate::errors::{map_connect_error, map_error};
use crate::introspect::{self, Flavour};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_USER: &str = "root";
const QUERY_INTERRUPTED: u16 = 1317;

pub(crate) struct MysqlConnection {
    conn: Mutex<Option<Conn>>,
    cancel: Arc<MysqlCancel>,
    /// Detected on first introspection.
    flavour: OnceLock<Flavour>,
}

impl MysqlConnection {
    pub(crate) async fn open(config: &ConnectionConfig) -> Result<Self, DbError> {
        let opts = opts(config, ssl_opts(&config.tls, config)?)?;
        let conn = match connect(opts.clone()).await {
            // `prefer`: fall back to plain text when the server has no TLS.
            Err(Error::Driver(DriverError::NoClientSslFlagFromServer))
                if config.tls.mode == TlsMode::Prefer =>
            {
                connect(opts_without_tls(&opts)).await
            }
            other => other,
        }
        .map_err(|e| map_connect_error(&e))?;
        Self::finish_open(conn, config.read_only).await
    }

    async fn finish_open(mut conn: Conn, read_only: bool) -> Result<Self, DbError> {
        if read_only {
            conn.query_drop("SET SESSION TRANSACTION READ ONLY")
                .await
                .map_err(|e| map_connect_error(&e))?;
        }
        let cancel = Arc::new(MysqlCancel {
            opts: conn.opts().clone(),
            connection_id: conn.id(),
            requested: AtomicBool::new(false),
        });
        Ok(Self {
            conn: Mutex::new(Some(conn)),
            cancel,
            flavour: OnceLock::new(),
        })
    }

    async fn run(
        &self,
        conn: &mut Conn,
        statement: &str,
        limit: usize,
    ) -> Result<StatementOutcome, DbError> {
        let result = conn
            .query_iter(statement)
            .await
            .map_err(|e| map_error(&e))?;
        self.read(result, limit).await
    }

    /// A prepared statement with parameters, over the binary protocol.
    async fn run_params(
        &self,
        conn: &mut Conn,
        statement: &str,
        params: Vec<MyValue>,
        limit: usize,
    ) -> Result<StatementOutcome, DbError> {
        let params = if params.is_empty() {
            Params::Empty
        } else {
            Params::Positional(params)
        };
        let result = conn
            .exec_iter(statement, params)
            .await
            .map_err(|e| map_error(&e))?;
        self.read(result, limit).await
    }

    async fn read<P: Protocol>(
        &self,
        mut result: QueryResult<'_, 'static, P>,
        limit: usize,
    ) -> Result<StatementOutcome, DbError> {
        let columns = result.columns().filter(|c| !c.is_empty());
        let Some(columns) = columns else {
            let rows = result.affected_rows();
            result.drop_result().await.map_err(|e| map_error(&e))?;
            return Ok(StatementOutcome::Affected { rows });
        };
        let mut rows: Vec<Vec<Value>> = Vec::new();
        let mut truncated = false;
        while let Some(row) = result.next().await.map_err(|e| map_error(&e))? {
            if rows.len() == limit {
                truncated = true;
                break;
            }
            rows.push(
                columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| decode(c, row.as_ref(i)))
                    .collect(),
            );
        }
        if truncated {
            self.stop_early(&mut result).await?;
        }
        result.drop_result().await.map_err(|e| map_error(&e))?;
        let columns = columns.iter().map(column_meta).collect();
        Ok(StatementOutcome::Rows(ResultSet {
            columns,
            rows,
            truncated,
        }))
    }
}

impl MysqlConnection {
    /// D-017: KILL QUERY first, then read what is left of the response.
    async fn stop_early<P: Protocol>(
        &self,
        result: &mut QueryResult<'_, 'static, P>,
    ) -> Result<(), DbError> {
        let killed = self.cancel.kill().await;
        loop {
            match result.next().await {
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(Error::Server(e)) if e.code == QUERY_INTERRUPTED => break,
                Err(e) => return Err(map_error(&e)),
            }
        }
        killed
    }

    /// Sends the rows of a prepared statement in batches (`Connection::stream_rows`).
    async fn stream(
        &self,
        conn: &mut Conn,
        statement: &str,
        params: Vec<MyValue>,
        rows: mpsc::Sender<StreamEvent>,
    ) -> Result<u64, DbError> {
        let params = if params.is_empty() {
            Params::Empty
        } else {
            Params::Positional(params)
        };
        let mut result = conn
            .exec_iter(statement, params)
            .await
            .map_err(|e| map_error(&e))?;
        let columns = result.columns().unwrap_or_default();
        if rows
            .send(StreamEvent::Columns(
                columns.iter().map(column_meta).collect(),
            ))
            .await
            .is_err()
        {
            result.drop_result().await.map_err(|e| map_error(&e))?;
            return Ok(0);
        }
        let mut sent = 0u64;
        let mut batch = Vec::with_capacity(STREAM_BATCH);
        while let Some(row) = result.next().await.map_err(|e| map_error(&e))? {
            batch.push(
                columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| decode(c, row.as_ref(i)))
                    .collect(),
            );
            if batch.len() == STREAM_BATCH {
                let full = std::mem::replace(&mut batch, Vec::with_capacity(STREAM_BATCH));
                let count = full.len() as u64;
                if rows.send(StreamEvent::Rows(full)).await.is_err() {
                    self.stop_early(&mut result).await?;
                    result.drop_result().await.map_err(|e| map_error(&e))?;
                    return Ok(sent);
                }
                sent += count;
            }
        }
        result.drop_result().await.map_err(|e| map_error(&e))?;
        let count = batch.len() as u64;
        if !batch.is_empty() && rows.send(StreamEvent::Rows(batch)).await.is_ok() {
            sent += count;
        }
        Ok(sent)
    }

    async fn flavour(&self, conn: &mut Conn) -> Result<Flavour, DbError> {
        if let Some(flavour) = self.flavour.get() {
            return Ok(*flavour);
        }
        let flavour = Flavour::detect(conn).await?;
        Ok(*self.flavour.get_or_init(|| flavour))
    }
}

async fn connect(opts: Opts) -> Result<Conn, Error> {
    match tokio::time::timeout(CONNECT_TIMEOUT, Conn::new(opts)).await {
        Ok(result) => result,
        Err(_) => Err(Error::Io(
            std::io::Error::from(std::io::ErrorKind::TimedOut).into(),
        )),
    }
}

fn opts(config: &ConnectionConfig, ssl: Option<SslOpts>) -> Result<Opts, DbError> {
    let Endpoint::Tcp {
        dial_host,
        dial_port,
        ..
    } = &config.endpoint
    else {
        return Err(DbError::new(
            DbErrorCode::Connect,
            "MySQL connections need a host and port",
        ));
    };
    let builder = OptsBuilder::default()
        .ip_or_hostname(dial_host.clone())
        .tcp_port(*dial_port)
        .prefer_socket(false)
        // Report matched rows, like PostgreSQL and SQLite; edits rely on "exactly one row".
        .client_found_rows(true)
        .user(Some(
            config
                .user
                .clone()
                .unwrap_or_else(|| DEFAULT_USER.to_owned()),
        ))
        .pass(
            config
                .password
                .as_ref()
                .map(|p| p.expose_secret().to_owned()),
        )
        .db_name(config.database.clone())
        .ssl_opts(ssl);
    Ok(builder.into())
}

fn opts_without_tls(opts: &Opts) -> Opts {
    OptsBuilder::from_opts(opts.clone()).ssl_opts(None).into()
}

/// `prefer`/`require` skip verification; `verify-ca` checks the chain; `verify-full` also the
/// real server name, which differs from the dialled address through a tunnel (D-004).
fn ssl_opts(tls: &TlsConfig, config: &ConnectionConfig) -> Result<Option<SslOpts>, DbError> {
    let base = SslOpts::default();
    let ssl = match tls.mode {
        TlsMode::Disable => return Ok(None),
        TlsMode::Prefer | TlsMode::Require => base
            .with_danger_accept_invalid_certs(true)
            .with_danger_skip_domain_validation(true),
        TlsMode::VerifyCa => {
            return Err(DbError::new(
                DbErrorCode::Tls,
                "verify-ca is not supported for MySQL; use verify-full or require",
            ));
        }
        TlsMode::VerifyFull => match &config.endpoint {
            Endpoint::Tcp { server_name, .. } => {
                base.with_danger_tls_hostname_override(Some(server_name.clone()))
            }
            Endpoint::File { .. } => base,
        },
    };
    let ssl = match &tls.ca_file {
        Some(ca) => ssl.with_root_certs(vec![ca.clone().into()]),
        None => ssl,
    };
    match (&tls.client_cert_file, &tls.client_key_file) {
        (Some(cert), Some(key)) => Ok(Some(ssl.with_client_identity(Some(ClientIdentity::new(
            cert.clone().into(),
            key.clone().into(),
        ))))),
        (None, None) => Ok(Some(ssl)),
        _ => Err(DbError::new(
            DbErrorCode::Tls,
            "A client certificate needs both a certificate and a key file",
        )),
    }
}

#[async_trait]
impl Connection for MysqlConnection {
    async fn ping(&self) -> Result<(), DbError> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(closed)?;
        conn.ping().await.map_err(|e| map_error(&e))
    }

    async fn server_version(&self) -> Result<String, DbError> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(closed)?;
        let version: Option<String> = conn
            .query_first("SELECT VERSION()")
            .await
            .map_err(|e| map_error(&e))?;
        let version = version.unwrap_or_default();
        Ok(match version.split_once("-MariaDB") {
            Some((number, _)) => format!("MariaDB {number}"),
            None => format!("MySQL {version}"),
        })
    }

    async fn execute(
        &self,
        statement: &str,
        options: &ExecuteOptions,
    ) -> Result<StatementOutcome, DbError> {
        // The client always enables multi-statements, so refuse them here.
        if sql::split(statement, crate::rules()).len() > 1 {
            return Err(DbError::new(
                DbErrorCode::Syntax,
                "Only one statement can be executed at a time",
            ));
        }
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(closed)?;
        self.cancel.requested.store(false, Ordering::SeqCst);
        let outcome = self.run(conn, statement, options.row_limit).await?;
        // MySQL lets a killed SLEEP() or BENCHMARK() return a row instead of an error; rows from
        // an interrupted query are incomplete, so they are reported as cancelled.
        if matches!(outcome, StatementOutcome::Rows(_))
            && self.cancel.requested.load(Ordering::SeqCst)
        {
            return Err(DbError::new(
                DbErrorCode::Cancelled,
                "The statement was cancelled",
            ));
        }
        Ok(outcome)
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>, DbError> {
        let mut guard = self.conn.lock().await;
        introspect::list_namespaces(guard.as_mut().ok_or_else(closed)?).await
    }

    async fn list_objects(&self, namespace: &NamespaceRef) -> Result<Vec<ObjectSummary>, DbError> {
        let mut guard = self.conn.lock().await;
        introspect::list_objects(guard.as_mut().ok_or_else(closed)?, namespace).await
    }

    async fn describe_table(&self, table: &TableRef) -> Result<TableInfo, DbError> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(closed)?;
        let flavour = self.flavour(conn).await?;
        introspect::describe_table(conn, flavour, table).await
    }

    async fn default_namespace(&self) -> Result<Option<NamespaceRef>, DbError> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(closed)?;
        let database: Option<Option<String>> = conn
            .query_first("SELECT DATABASE()")
            .await
            .map_err(|e| map_error(&e))?;
        Ok(database.flatten().map(|schema| NamespaceRef {
            catalog: None,
            schema: Some(schema),
        }))
    }

    async fn completion_schema(
        &self,
        namespace: &NamespaceRef,
    ) -> Result<CompletionSchema, DbError> {
        let mut guard = self.conn.lock().await;
        crate::completion::completion_schema(guard.as_mut().ok_or_else(closed)?, namespace).await
    }

    async fn table_ddl(&self, table: &TableRef) -> Result<String, DbError> {
        let mut guard = self.conn.lock().await;
        introspect::table_ddl(guard.as_mut().ok_or_else(closed)?, table).await
    }

    /// `FORMAT=TREE` where the server has it. MariaDB and older MySQL refuse that format, and
    /// get the classic table instead; when that fails too, its error is the one reported, as
    /// it describes the statement rather than the format.
    async fn explain(&self, statement: &str) -> Result<dbc_core::plan::PlanNode, DbError> {
        let body = dbc_core::plan::statement_body(statement);
        let options = ExecuteOptions::default();
        if let Ok(outcome) = self
            .execute(&format!("EXPLAIN FORMAT=TREE {body}"), &options)
            .await
        {
            return Ok(dbc_core::plan::from_indented(
                &dbc_core::plan::column_text(&outcome, 0).join("\n"),
            ));
        }
        match self.execute(&format!("EXPLAIN {body}"), &options).await? {
            StatementOutcome::Rows(result) => Ok(crate::plan::from_table(&result)),
            StatementOutcome::Affected { .. } => Err(DbError::new(
                dbc_core::DbErrorCode::Internal,
                "EXPLAIN returned no plan",
            )),
        }
    }

    async fn query_params(
        &self,
        statement: &str,
        params: &[Value],
        options: &ExecuteOptions,
    ) -> Result<StatementOutcome, DbError> {
        let params = params.iter().map(to_mysql).collect::<Result<Vec<_>, _>>()?;
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(closed)?;
        self.cancel.requested.store(false, Ordering::SeqCst);
        self.run_params(conn, statement, params, options.row_limit)
            .await
    }

    async fn stream_rows(
        &self,
        statement: &str,
        params: &[Value],
        rows: mpsc::Sender<StreamEvent>,
    ) -> Result<u64, DbError> {
        let params = params.iter().map(to_mysql).collect::<Result<Vec<_>, _>>()?;
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(closed)?;
        self.cancel.requested.store(false, Ordering::SeqCst);
        self.stream(conn, statement, params, rows).await
    }

    async fn estimate_rows(&self, table: &TableRef) -> Result<Option<u64>, DbError> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(closed)?;
        // TABLE_ROWS is InnoDB's estimate; NULL for views.
        let estimate: Option<Option<u64>> = conn
            .exec_first(
                "SELECT TABLE_ROWS FROM information_schema.TABLES \
                 WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND TABLE_TYPE = 'BASE TABLE'",
                (
                    table.namespace.schema.clone().unwrap_or_default(),
                    table.name.clone(),
                ),
            )
            .await
            .map_err(|e| map_error(&e))?;
        Ok(estimate.flatten())
    }

    fn cancel_handle(&self) -> Arc<dyn CancelHandle> {
        Arc::clone(&self.cancel) as Arc<dyn CancelHandle>
    }

    /// Reads `SHOW CREATE TABLE`, then plans without the server (D-042).
    async fn plan_schema_edit(
        &self,
        edit: &dbc_core::schema_edit::SchemaEdit,
    ) -> Result<dbc_core::schema_edit::SchemaPlan, DbError> {
        let create = self.table_ddl(&edit.original.table).await?;
        let create = create.trim_end().trim_end_matches(';');
        crate::schema_edit::plan_schema_edit(edit, create)
    }

    async fn close(&self) -> Result<(), DbError> {
        let conn = self.conn.lock().await.take();
        match conn {
            Some(conn) => conn.disconnect().await.map_err(|e| map_error(&e)),
            None => Ok(()),
        }
    }
}

/// `KILL QUERY <id>` from a short-lived side connection.
struct MysqlCancel {
    opts: Opts,
    connection_id: u32,
    /// Set when the user cancels, so an interrupted query that still returned rows is reported.
    requested: AtomicBool,
}

#[async_trait]
impl CancelHandle for MysqlCancel {
    async fn cancel(&self) -> Result<(), DbError> {
        self.requested.store(true, Ordering::SeqCst);
        self.kill().await
    }
}

impl MysqlCancel {
    async fn kill(&self) -> Result<(), DbError> {
        let mut side = connect(self.opts.clone())
            .await
            .map_err(|e| map_connect_error(&e))?;
        let killed = side
            .query_drop(format!("KILL QUERY {}", self.connection_id))
            .await
            .map_err(|e| map_error(&e));
        let disconnected = side.disconnect().await.map_err(|e| map_error(&e));
        killed.and(disconnected)
    }
}

/// Binds a value natively; text-like values go as strings, which MySQL converts.
fn to_mysql(value: &Value) -> Result<MyValue, DbError> {
    Ok(match value {
        Value::Null => MyValue::NULL,
        Value::Bool(b) => MyValue::Int(i64::from(*b)),
        Value::Int(i) => MyValue::Int(*i),
        Value::UInt(u) => MyValue::UInt(*u),
        Value::Float(f) => MyValue::Double(*f),
        Value::Bytes(bytes) => MyValue::Bytes(bytes.clone()),
        Value::Decimal(s)
        | Value::Text(s)
        | Value::Json(s)
        | Value::Xml(s)
        | Value::Uuid(s)
        | Value::Date(s)
        | Value::Time(s)
        | Value::Interval(s) => MyValue::Bytes(s.clone().into_bytes()),
        // MySQL datetimes have a space where ISO 8601 has `T`.
        Value::Timestamp(s) | Value::TimestampTz(s) => {
            MyValue::Bytes(s.replacen('T', " ", 1).into_bytes())
        }
        Value::Unknown {
            text: Some(text), ..
        } => MyValue::Bytes(text.clone().into_bytes()),
        Value::Array(_) | Value::Unknown { text: None, .. } => {
            return Err(DbError::new(
                DbErrorCode::Unsupported,
                "Arrays and values without a text form cannot be used as parameters yet",
            ));
        }
    })
}

fn closed() -> DbError {
    DbError::new(DbErrorCode::ConnectionLost, "The connection is closed")
}
