//! A PostgreSQL connection: connect, execute with the row limit (D-017), cancel, close.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use dbc_core::driver::{CancelHandle, Connection, ConnectionConfig, Endpoint, TlsMode};
use dbc_core::result::{
    ColumnMeta, ExecuteOptions, ResultSet, STREAM_BATCH, StatementOutcome, StreamEvent,
};
use dbc_core::schema::{
    CompletionSchema, Namespace, NamespaceRef, ObjectSummary, TableInfo, TableRef,
};
use dbc_core::{DbError, DbErrorCode, Value};
use futures_util::{StreamExt, pin_mut};
use rustls::ClientConfig;
use secrecy::ExposeSecret;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_postgres::config::SslMode;
use tokio_postgres::tls::MakeTlsConnect;
use tokio_postgres::types::{ToSql, Type};
use tokio_postgres::{CancelToken, Client, NoTls, RowStream, Statement};
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::decode_row;
use crate::errors::{map_connect_error, map_error};

/// The TLS connector type `tokio-postgres-rustls` produces (its concrete type is private).
type RustlsConnect = <MakeRustlsConnect as MakeTlsConnect<TcpStream>>::TlsConnect;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const APPLICATION_NAME: &str = "Dunlin";
const DEFAULT_USER: &str = "postgres";
const SQLSTATE_CANCELLED: &str = "57014";
/// After the row limit, rows still read before deciding to cancel (D-017).
const GRACE_ROWS: usize = 10_000;
const GRACE_TIME: Duration = Duration::from_millis(50);
/// Long enough for a cancel already sent to arrive.
const LATE_CANCEL_GUARD: &str = "SELECT pg_sleep(0.1)";

/// Where and how to open a socket; shared by the connection and its cancel handle.
#[derive(Clone)]
struct Dialer {
    host: String,
    port: u16,
    server_name: String,
    tls: Option<Arc<ClientConfig>>,
}

impl Dialer {
    async fn socket(&self) -> Result<TcpStream, DbError> {
        let stream = TcpStream::connect((self.host.as_str(), self.port))
            .await
            .map_err(|e| {
                DbError::new(
                    DbErrorCode::Connect,
                    format!("Cannot reach {}:{}: {e}", self.host, self.port),
                )
            })?;
        // A latency hint only; the connection works without it.
        let _ = stream.set_nodelay(true);
        Ok(stream)
    }

    /// TLS for the real server name, never the dialled address (DECISIONS D-004).
    fn tls_connect(&self, config: &Arc<ClientConfig>) -> Result<RustlsConnect, DbError> {
        let mut make = MakeRustlsConnect::new(ClientConfig::clone(config));
        <MakeRustlsConnect as MakeTlsConnect<TcpStream>>::make_tls_connect(
            &mut make,
            &self.server_name,
        )
        .map_err(|e| match e {})
    }
}

pub(crate) struct PostgresConnection {
    client: Mutex<Option<Arc<Client>>>,
    cancel: Arc<PostgresCancel>,
}

impl PostgresConnection {
    pub(crate) async fn open(config: &ConnectionConfig) -> Result<Self, DbError> {
        let Endpoint::Tcp {
            dial_host,
            dial_port,
            server_name,
        } = &config.endpoint
        else {
            return Err(DbError::new(
                DbErrorCode::Connect,
                "PostgreSQL connections need a host and port",
            ));
        };
        let dialer = Dialer {
            host: dial_host.clone(),
            port: *dial_port,
            server_name: server_name.clone(),
            tls: crate::tls::client_config(&config.tls)?.map(Arc::new),
        };
        let client = tokio::time::timeout(CONNECT_TIMEOUT, handshake(&pg_config(config), &dialer))
            .await
            .map_err(|_| {
                DbError::new(DbErrorCode::Timeout, "Connecting to the server timed out")
            })??;
        if config.read_only {
            client
                .batch_execute("SET default_transaction_read_only = on")
                .await
                .map_err(|e| map_connect_error(&e))?;
        }
        let cancel = Arc::new(PostgresCancel {
            token: client.cancel_token(),
            dialer,
        });
        Ok(Self {
            client: Mutex::new(Some(Arc::new(client))),
            cancel,
        })
    }

    fn client(&self) -> Result<Arc<Client>, DbError> {
        let guard = self.client.lock().map_err(|_| poisoned())?;
        let client = guard.as_ref().ok_or_else(closed)?;
        if client.is_closed() {
            return Err(DbError::new(
                DbErrorCode::ConnectionLost,
                "The connection to the server was lost",
            ));
        }
        Ok(Arc::clone(client))
    }

    async fn read_rows(
        &self,
        client: &Client,
        statement: &Statement,
        params: &[Option<String>],
        sql: &str,
        limit: usize,
    ) -> Result<ResultSet, DbError> {
        let stream = client
            .query_raw(statement, params.iter().map(|p| p as &(dyn ToSql + Sync)))
            .await
            .map_err(|e| map_error(&e, sql))?;
        pin_mut!(stream);
        let mut rows: Vec<Vec<Value>> = Vec::new();
        let mut truncated = false;
        while let Some(row) = stream.next().await {
            let row = row.map_err(|e| map_error(&e, sql))?;
            if rows.len() == limit {
                truncated = true;
                break;
            }
            rows.push(decode_row(&row));
        }
        if truncated {
            self.stop_early(client, stream.as_mut(), sql).await?;
        }
        let columns = statement.columns().iter().map(column_meta).collect();
        Ok(ResultSet {
            columns,
            rows,
            truncated,
        })
    }
}

impl PostgresConnection {
    /// D-017: a statement that is nearly done is read to the end; a longer one is cancelled.
    async fn stop_early(
        &self,
        client: &Client,
        mut stream: Pin<&mut RowStream>,
        sql: &str,
    ) -> Result<(), DbError> {
        let drained = tokio::time::timeout(GRACE_TIME, async {
            for _ in 0..GRACE_ROWS {
                match stream.next().await {
                    None => return Ok(true),
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(map_error(&e, sql)),
                }
            }
            Ok(false)
        })
        .await;
        if drained.unwrap_or(Ok(false))? {
            return Ok(());
        }
        // Wait for the cancel, then drain what the server already sent.
        let cancelled = self.cancel.cancel().await;
        let mut interrupted = false;
        while let Some(rest) = stream.next().await {
            match rest {
                Ok(_) => {}
                Err(e) if is_cancel(&e) => {
                    interrupted = true;
                    break;
                }
                Err(e) => return Err(map_error(&e, sql)),
            }
        }
        cancelled?;
        if interrupted {
            return Ok(());
        }
        absorb_late_cancel(client).await
    }
}

/// The statement finished just before the cancel reached it. The server may still deliver the
/// cancel, which would interrupt the next statement; this statement is there to take it.
async fn absorb_late_cancel(client: &Client) -> Result<(), DbError> {
    match client.simple_query(LATE_CANCEL_GUARD).await {
        Ok(_) => Ok(()),
        Err(e) if is_cancel(&e) => Ok(()),
        Err(e) => Err(map_error(&e, "")),
    }
}

fn pg_config(config: &ConnectionConfig) -> tokio_postgres::Config {
    let mut pg = tokio_postgres::Config::new();
    pg.user(config.user.as_deref().unwrap_or(DEFAULT_USER));
    if let Some(password) = &config.password {
        pg.password(password.expose_secret());
    }
    if let Some(database) = &config.database {
        pg.dbname(database);
    }
    pg.application_name(APPLICATION_NAME);
    pg.ssl_mode(match config.tls.mode {
        TlsMode::Disable => SslMode::Disable,
        TlsMode::Prefer => SslMode::Prefer,
        TlsMode::Require | TlsMode::VerifyCa | TlsMode::VerifyFull => SslMode::Require,
    });
    pg
}

async fn handshake(pg: &tokio_postgres::Config, dialer: &Dialer) -> Result<Client, DbError> {
    let stream = dialer.socket().await?;
    match &dialer.tls {
        None => {
            let (client, connection) = pg
                .connect_raw(stream, NoTls)
                .await
                .map_err(|e| map_connect_error(&e))?;
            tokio::spawn(connection);
            Ok(client)
        }
        Some(config) => {
            let tls = dialer.tls_connect(config)?;
            let (client, connection) = pg
                .connect_raw(stream, tls)
                .await
                .map_err(|e| map_connect_error(&e))?;
            tokio::spawn(connection);
            Ok(client)
        }
    }
}

fn column_meta(column: &tokio_postgres::Column) -> ColumnMeta {
    ColumnMeta {
        name: column.name().to_owned(),
        type_name: column.type_().name().to_owned(),
        category: crate::decode::category(column.type_()),
        nullable: None,
    }
}

fn is_cancel(error: &tokio_postgres::Error) -> bool {
    error
        .as_db_error()
        .is_some_and(|db| db.code().code() == SQLSTATE_CANCELLED)
}

#[async_trait]
impl Connection for PostgresConnection {
    async fn ping(&self) -> Result<(), DbError> {
        let client = self.client()?;
        client
            .simple_query("SELECT 1")
            .await
            .map(|_| ())
            .map_err(|e| map_error(&e, ""))
    }

    async fn server_version(&self) -> Result<String, DbError> {
        let client = self.client()?;
        let row = client
            .query_one("SHOW server_version", &[])
            .await
            .map_err(|e| map_error(&e, ""))?;
        let version: String = row.try_get(0).map_err(|e| map_error(&e, ""))?;
        Ok(format!("PostgreSQL {version}"))
    }

    async fn execute(
        &self,
        sql: &str,
        options: &ExecuteOptions,
    ) -> Result<StatementOutcome, DbError> {
        let client = self.client()?;
        let statement = client.prepare(sql).await.map_err(|e| map_error(&e, sql))?;
        if statement.columns().is_empty() {
            let rows = client
                .execute(&statement, &[])
                .await
                .map_err(|e| map_error(&e, sql))?;
            return Ok(StatementOutcome::Affected { rows });
        }
        let result = self
            .read_rows(&client, &statement, &[], sql, options.row_limit)
            .await?;
        Ok(StatementOutcome::Rows(result))
    }

    async fn query_params(
        &self,
        sql: &str,
        params: &[Value],
        options: &ExecuteOptions,
    ) -> Result<StatementOutcome, DbError> {
        let client = self.client()?;
        let texts = params
            .iter()
            .map(crate::params::as_text)
            .collect::<Result<Vec<_>, _>>()?;
        let types = vec![Type::TEXT; texts.len()];
        let statement = client
            .prepare_typed(sql, &types)
            .await
            .map_err(|e| map_error(&e, sql))?;
        if statement.columns().is_empty() {
            let refs: Vec<&(dyn ToSql + Sync)> =
                texts.iter().map(|t| t as &(dyn ToSql + Sync)).collect();
            let rows = client
                .execute(&statement, &refs)
                .await
                .map_err(|e| map_error(&e, sql))?;
            return Ok(StatementOutcome::Affected { rows });
        }
        let result = self
            .read_rows(&client, &statement, &texts, sql, options.row_limit)
            .await?;
        Ok(StatementOutcome::Rows(result))
    }

    async fn stream_rows(
        &self,
        sql: &str,
        params: &[Value],
        rows: mpsc::Sender<StreamEvent>,
    ) -> Result<u64, DbError> {
        let client = self.client()?;
        let texts = params
            .iter()
            .map(crate::params::as_text)
            .collect::<Result<Vec<_>, _>>()?;
        let statement = client
            .prepare_typed(sql, &vec![Type::TEXT; texts.len()])
            .await
            .map_err(|e| map_error(&e, sql))?;
        let columns = statement.columns().iter().map(column_meta).collect();
        if rows.send(StreamEvent::Columns(columns)).await.is_err() {
            return Ok(0);
        }
        let stream = client
            .query_raw(&statement, texts.iter().map(|p| p as &(dyn ToSql + Sync)))
            .await
            .map_err(|e| map_error(&e, sql))?;
        pin_mut!(stream);
        let mut sent = 0u64;
        let mut batch = Vec::with_capacity(STREAM_BATCH);
        while let Some(row) = stream.next().await {
            batch.push(decode_row(&row.map_err(|e| map_error(&e, sql))?));
            if batch.len() == STREAM_BATCH {
                let full = std::mem::replace(&mut batch, Vec::with_capacity(STREAM_BATCH));
                let count = full.len() as u64;
                if rows.send(StreamEvent::Rows(full)).await.is_err() {
                    // Nobody reads any more: stop the statement rather than drain it.
                    self.stop_early(&client, stream.as_mut(), sql).await?;
                    return Ok(sent);
                }
                sent += count;
            }
        }
        let count = batch.len() as u64;
        if !batch.is_empty() && rows.send(StreamEvent::Rows(batch)).await.is_ok() {
            sent += count;
        }
        Ok(sent)
    }

    async fn estimate_rows(&self, table: &TableRef) -> Result<Option<u64>, DbError> {
        let client = self.client()?;
        let schema = table.namespace.schema.as_deref().unwrap_or("public");
        // reltuples is -1 before the first VACUUM or ANALYZE; views have no statistics.
        let rows = client
            .query(
                "SELECT c.reltuples::bigint FROM pg_class c \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r', 'p', 'm', 'f')",
                &[&schema, &table.name],
            )
            .await
            .map_err(|e| map_error(&e, ""))?;
        let estimate = match rows.first() {
            Some(row) => row.try_get::<_, i64>(0).map_err(|e| map_error(&e, ""))?,
            None => return Ok(None),
        };
        Ok(u64::try_from(estimate).ok())
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>, DbError> {
        crate::introspect::list_namespaces(&*self.client()?).await
    }

    async fn list_objects(&self, namespace: &NamespaceRef) -> Result<Vec<ObjectSummary>, DbError> {
        crate::introspect::list_objects(&*self.client()?, namespace).await
    }

    async fn describe_table(&self, table: &TableRef) -> Result<TableInfo, DbError> {
        crate::introspect::describe_table(&*self.client()?, table).await
    }

    async fn default_namespace(&self) -> Result<Option<NamespaceRef>, DbError> {
        let client = self.client()?;
        let row = client
            .query_one("SELECT current_schema()::text", &[])
            .await
            .map_err(|e| map_error(&e, ""))?;
        let schema: Option<String> = row.try_get(0).map_err(|e| map_error(&e, ""))?;
        Ok(schema.map(|schema| NamespaceRef {
            catalog: None,
            schema: Some(schema),
        }))
    }

    async fn completion_schema(
        &self,
        namespace: &NamespaceRef,
    ) -> Result<CompletionSchema, DbError> {
        crate::completion::completion_schema(&*self.client()?, namespace).await
    }

    async fn table_ddl(&self, table: &TableRef) -> Result<String, DbError> {
        crate::ddl::table_ddl(&*self.client()?, table).await
    }

    async fn explain(&self, statement: &str) -> Result<dbc_core::plan::PlanNode, DbError> {
        let sql = format!(
            "EXPLAIN (FORMAT JSON) {}",
            dbc_core::plan::statement_body(statement)
        );
        let outcome = self.execute(&sql, &ExecuteOptions::default()).await?;
        let text = dbc_core::plan::column_text(&outcome, 0).concat();
        crate::plan::from_json(&text)
    }

    fn cancel_handle(&self) -> Arc<dyn CancelHandle> {
        Arc::clone(&self.cancel) as Arc<dyn CancelHandle>
    }

    /// `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)`; with `rollback`, inside a transaction that is
    /// rolled back whatever happens.
    async fn explain_analyze(
        &self,
        statement: &str,
        rollback: bool,
    ) -> Result<dbc_core::plan::AnalyzedPlan, DbError> {
        let sql = format!(
            "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {}",
            dbc_core::plan::statement_body(statement)
        );
        let options = ExecuteOptions::default();
        if rollback {
            self.execute("BEGIN", &options).await?;
        }
        let outcome = self.execute(&sql, &options).await;
        if rollback {
            let undone = self.execute("ROLLBACK", &options).await;
            // The statement's own error is the one to report; a failed rollback otherwise.
            if outcome.is_ok() {
                undone?;
            }
        }
        let text = dbc_core::plan::column_text(&outcome?, 0).concat();
        crate::plan::analyzed_from_json(&text)
    }

    async fn plan_schema_edit(
        &self,
        edit: &dbc_core::schema_edit::SchemaEdit,
    ) -> Result<dbc_core::schema_edit::SchemaPlan, DbError> {
        crate::schema_edit::plan_schema_edit(edit)
    }

    async fn close(&self) -> Result<(), DbError> {
        // Dropping the last client handle ends the connection task, which closes the socket.
        let client = self.client.lock().map_err(|_| poisoned())?.take();
        drop(client);
        Ok(())
    }
}

struct PostgresCancel {
    token: CancelToken,
    dialer: Dialer,
}

#[async_trait]
impl CancelHandle for PostgresCancel {
    /// Sends a cancel request on a side connection and waits until the server has taken it.
    async fn cancel(&self) -> Result<(), DbError> {
        let stream = self.dialer.socket().await?;
        let result = match &self.dialer.tls {
            None => self.token.cancel_query_raw(stream, NoTls).await,
            Some(config) => {
                self.token
                    .cancel_query_raw(stream, self.dialer.tls_connect(config)?)
                    .await
            }
        };
        result.map_err(|e| map_connect_error(&e))
    }
}

fn closed() -> DbError {
    DbError::new(DbErrorCode::ConnectionLost, "The connection is closed")
}

fn poisoned() -> DbError {
    DbError::new(
        DbErrorCode::Internal,
        "Connection state is unavailable after a panic",
    )
}
