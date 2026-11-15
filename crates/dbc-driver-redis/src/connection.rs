//! One Redis connection: a multiplexed RESP connection over a socket the driver opens itself,
//! so TLS verifies the real server name even through an SSH tunnel (D-004).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use dbc_core::driver::{CancelHandle, Connection, ConnectionConfig, Endpoint};
use dbc_core::result::{ExecuteOptions, StatementOutcome};
use dbc_core::{DbError, DbErrorCode};
use redis::AsyncConnectionConfig;
use redis::aio::MultiplexedConnection;
use redis::{ProtocolVersion, RedisConnectionInfo, Value as Reply};
use secrecy::ExposeSecret;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::task::AbortHandle;

use crate::command::{self, UNSUPPORTED};
use crate::errors::{map_connect_error, map_error};
use crate::reply;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

pub struct RedisConnection {
    conn: MultiplexedConnection,
    cancel: Arc<RedisCancel>,
}

/// Redis cannot stop a running command for one client: cancelling closes the connection, and
/// the session opens a new one for the next command.
struct RedisCancel {
    driver: AbortHandle,
    closed: AtomicBool,
}

#[async_trait]
impl CancelHandle for RedisCancel {
    async fn cancel(&self) -> Result<(), DbError> {
        self.closed.store(true, Ordering::SeqCst);
        self.driver.abort();
        Ok(())
    }
}

/// The database number from the profile's database field: empty is 0.
pub fn database_index(database: Option<&str>) -> Result<i64, DbError> {
    match database.map(str::trim).filter(|d| !d.is_empty()) {
        None => Ok(0),
        Some(text) => text.parse::<i64>().ok().filter(|n| *n >= 0).ok_or_else(|| {
            DbError::new(
                DbErrorCode::Connect,
                "The Redis database must be a number, such as 0",
            )
        }),
    }
}

async fn open_stream(config: &ConnectionConfig) -> Result<Box<dyn Stream>, DbError> {
    let Endpoint::Tcp {
        dial_host,
        dial_port,
        server_name,
    } = &config.endpoint
    else {
        return Err(DbError::new(
            DbErrorCode::Connect,
            "Redis needs a host and port",
        ));
    };
    let tcp = tokio::time::timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((dial_host.as_str(), *dial_port)),
    )
    .await
    .map_err(|_| DbError::new(DbErrorCode::Timeout, "The server did not answer in time"))?
    .map_err(|e| DbError::new(DbErrorCode::Connect, e.to_string()))?;
    // Latency matters more than throughput for a console; failing here changes nothing.
    let _ = tcp.set_nodelay(true);
    let Some(tls) = crate::tls::client_config(&config.tls)? else {
        return Ok(Box::new(tcp));
    };
    let name = rustls::pki_types::ServerName::try_from(server_name.clone())
        .map_err(|e| DbError::new(DbErrorCode::Tls, e.to_string()))?;
    let stream = tokio_rustls::TlsConnector::from(Arc::new(tls))
        .connect(name, tcp)
        .await
        .map_err(|e| DbError::new(DbErrorCode::Tls, e.to_string()))?;
    Ok(Box::new(stream))
}

fn settings(
    config: &ConnectionConfig,
    protocol: ProtocolVersion,
) -> Result<RedisConnectionInfo, DbError> {
    let mut info = RedisConnectionInfo::default()
        .set_db(database_index(config.database.as_deref())?)
        .set_protocol(protocol);
    if let Some(user) = config.user.as_deref().filter(|u| !u.is_empty()) {
        info = info.set_username(user);
    }
    if let Some(password) = &config.password {
        info = info.set_password(password.expose_secret());
    }
    Ok(info)
}

impl RedisConnection {
    /// RESP3 (maps, doubles, booleans) where the server has it, RESP2 otherwise.
    pub async fn open(config: &ConnectionConfig) -> Result<Self, DbError> {
        match Self::open_with(config, ProtocolVersion::RESP3).await {
            Err(error) if error.code == DbErrorCode::Syntax => {
                Self::open_with(config, ProtocolVersion::RESP2).await
            }
            other => other,
        }
    }

    async fn open_with(
        config: &ConnectionConfig,
        protocol: ProtocolVersion,
    ) -> Result<Self, DbError> {
        let info = settings(config, protocol)?;
        let stream = open_stream(config).await?;
        let (conn, driver) =
            MultiplexedConnection::new_with_config(&info, stream, AsyncConnectionConfig::new())
                .await
                .map_err(|e| map_connect_error(&e))?;
        let driver = tokio::spawn(driver).abort_handle();
        Ok(Self {
            conn,
            cancel: Arc::new(RedisCancel {
                driver,
                closed: AtomicBool::new(false),
            }),
        })
    }

    async fn run(&self, args: Vec<Vec<u8>>) -> Result<Reply, DbError> {
        let Some((name, rest)) = args.split_first() else {
            return Err(DbError::new(
                DbErrorCode::Syntax,
                "There is no command to run",
            ));
        };
        let name = String::from_utf8_lossy(name).to_ascii_uppercase();
        if UNSUPPORTED.contains(&name.as_str()) {
            return Err(DbError::unsupported(&format!(
                "{name} (subscriptions and MONITOR take over the connection)"
            )));
        }
        let mut cmd = redis::cmd(&name);
        for arg in rest {
            cmd.arg(arg.as_slice());
        }
        let mut conn = self.conn.clone();
        match cmd.query_async::<Reply>(&mut conn).await {
            Ok(reply) => Ok(reply),
            Err(_) if self.cancel.closed.load(Ordering::SeqCst) => Err(DbError::new(
                DbErrorCode::Cancelled,
                "Cancelled: the connection was closed to stop the command",
            )),
            Err(error) => Err(map_error(&error)),
        }
    }

    /// Several commands in one round trip; their replies in order.
    pub async fn pipeline(&self, commands: Vec<Vec<Vec<u8>>>) -> Result<Vec<Reply>, DbError> {
        let mut pipe = redis::pipe();
        for args in &commands {
            let Some((name, rest)) = args.split_first() else {
                continue;
            };
            let command = pipe.cmd(&String::from_utf8_lossy(name));
            for arg in rest {
                command.arg(arg.as_slice());
            }
        }
        let mut conn = self.conn.clone();
        pipe.query_async::<Vec<Reply>>(&mut conn)
            .await
            .map_err(|e| map_error(&e))
    }

    /// One command given as arguments (the key browser and editors, which never quote).
    pub async fn command(&self, args: Vec<Vec<u8>>) -> Result<Reply, DbError> {
        self.run(args).await
    }
}

#[async_trait]
impl Connection for RedisConnection {
    async fn ping(&self) -> Result<(), DbError> {
        self.run(vec![b"PING".to_vec()]).await.map(|_| ())
    }

    async fn server_version(&self) -> Result<String, DbError> {
        let info = self.run(vec![b"INFO".to_vec(), b"server".to_vec()]).await?;
        let text = match reply::cell(info) {
            dbc_core::Value::Text(text) => text,
            other => format!("{other:?}"),
        };
        let field = |name: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(name))
                .map(|v| v.trim().to_owned())
        };
        Ok(match (field("valkey_version:"), field("redis_version:")) {
            (Some(valkey), _) => format!("Valkey {valkey}"),
            (None, Some(redis)) => format!("Redis {redis}"),
            (None, None) => "Redis".to_owned(),
        })
    }

    async fn execute(
        &self,
        sql: &str,
        options: &ExecuteOptions,
    ) -> Result<StatementOutcome, DbError> {
        let args = command::parse_args(sql.trim())
            .ok_or_else(|| DbError::new(DbErrorCode::Syntax, "A quoted argument is not closed"))?;
        let reply = self.run(args).await?;
        Ok(StatementOutcome::Rows(reply::to_result(
            reply,
            options.row_limit,
        )?))
    }

    fn cancel_handle(&self) -> Arc<dyn CancelHandle> {
        Arc::clone(&self.cancel) as Arc<dyn CancelHandle>
    }

    fn key_value(&self) -> Option<&dyn dbc_core::kv::KeyValueStore> {
        Some(self)
    }

    fn is_broken(&self) -> bool {
        self.cancel.closed.load(Ordering::SeqCst)
    }

    async fn close(&self) -> Result<(), DbError> {
        self.cancel.closed.store(true, Ordering::SeqCst);
        self.cancel.driver.abort();
        Ok(())
    }
}
