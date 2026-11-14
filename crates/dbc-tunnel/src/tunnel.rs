//! The tunnel: SSH session(s), a loopback listener, one forwarding task per accepted socket.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::{JoinHandle, JoinSet};

use crate::auth::authenticate;
use crate::config::{KEEPALIVE_MAX_MISSED, SshHost, TunnelConfig};
use crate::error::TunnelError;
use crate::handler::{Client, Rejection};
use crate::host_keys::HostKeyStore;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

pub struct Tunnel {
    local_addr: SocketAddr,
    /// The final hop, which carries the forwarded connections.
    session: Arc<Handle<Client>>,
    /// The jump host's session, kept alive while the tunnel is open.
    jump: Option<Handle<Client>>,
    lost: watch::Receiver<bool>,
    accept_task: JoinHandle<()>,
}

impl Tunnel {
    /// Connects (through the jump host, if any), authenticates and starts listening on
    /// `127.0.0.1:<free port>`.
    pub async fn open(
        config: &TunnelConfig,
        store: Arc<dyn HostKeyStore>,
    ) -> Result<Self, TunnelError> {
        let (lost_tx, lost) = watch::channel(false);
        let lost_tx = Arc::new(lost_tx);
        let ssh_config = Arc::new(client::Config {
            keepalive_interval: Some(config.keepalive),
            keepalive_max: KEEPALIVE_MAX_MISSED,
            ..client::Config::default()
        });
        let hop = Hop {
            config: ssh_config,
            store,
            lost: lost_tx,
        };
        let (session, jump) = match &config.jump {
            None => (hop.direct(&config.ssh).await?, None),
            Some(jump_host) => {
                let first = hop.direct(jump_host).await?;
                let target = format!("{}:{}", config.ssh.host, config.ssh.port);
                let channel = first
                    .channel_open_direct_tcpip(
                        config.ssh.host.clone(),
                        u32::from(config.ssh.port),
                        "127.0.0.1",
                        0,
                    )
                    .await
                    .map_err(|e| TunnelError::ForwardRefused {
                        target,
                        reason: e.to_string(),
                    })?;
                (
                    hop.over(channel.into_stream(), &config.ssh).await?,
                    Some(first),
                )
            }
        };
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|e| TunnelError::Internal(format!("cannot listen on 127.0.0.1: {e}")))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| TunnelError::Internal(e.to_string()))?;
        let session = Arc::new(session);
        let target = (config.target_host.clone(), config.target_port);
        let accept_task = tokio::spawn(accept_loop(
            listener,
            Arc::clone(&session),
            target,
            lost.clone(),
        ));
        Ok(Self {
            local_addr,
            session,
            jump,
            lost,
            accept_task,
        })
    }

    /// `127.0.0.1:<port>`: what the database driver dials.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// The SSH connection (or the jump host's) has dropped; the listener is closed.
    pub fn is_lost(&self) -> bool {
        *self.lost.borrow()
            || self.session.is_closed()
            || self.jump.as_ref().is_some_and(Handle::is_closed)
    }

    /// Stops listening, ends every forwarded connection, then disconnects from the server(s).
    pub async fn close(self) -> Result<(), TunnelError> {
        self.accept_task.abort();
        let mut result = disconnect(&self.session).await;
        if let Some(jump) = &self.jump {
            result = result.and(disconnect(jump).await);
        }
        // Disconnecting an SSH session that already dropped is not a failure of `close`.
        if self.is_lost() { Ok(()) } else { result }
    }
}

async fn disconnect(handle: &Handle<Client>) -> Result<(), TunnelError> {
    handle
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await
        .map_err(|e| TunnelError::Internal(e.to_string()))
}

impl Drop for Tunnel {
    /// A dropped tunnel never leaves a listener behind.
    fn drop(&mut self) {
        self.accept_task.abort();
    }
}

struct Hop {
    config: Arc<client::Config>,
    store: Arc<dyn HostKeyStore>,
    lost: Arc<watch::Sender<bool>>,
}

impl Hop {
    fn client(&self, ssh: &SshHost) -> (Client, Rejection) {
        let rejection: Rejection = Arc::new(Mutex::new(None));
        let client = Client {
            host: ssh.host.clone(),
            port: ssh.port,
            store: Arc::clone(&self.store),
            rejection: Arc::clone(&rejection),
            lost: Arc::clone(&self.lost),
        };
        (client, rejection)
    }

    async fn direct(&self, ssh: &SshHost) -> Result<Handle<Client>, TunnelError> {
        let connect_error = |reason: String| TunnelError::Connect {
            host: ssh.host.clone(),
            port: ssh.port,
            reason,
        };
        let stream = tokio::time::timeout(
            CONNECT_TIMEOUT,
            TcpStream::connect((ssh.host.as_str(), ssh.port)),
        )
        .await
        .map_err(|_| connect_error("timed out".into()))?
        .map_err(|e| connect_error(e.to_string()))?;
        // A latency hint only.
        let _ = stream.set_nodelay(true);
        self.over(stream, ssh).await
    }

    /// SSH handshake and authentication over an established stream.
    async fn over<S>(&self, stream: S, ssh: &SshHost) -> Result<Handle<Client>, TunnelError>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (client, rejection) = self.client(ssh);
        let handshake = client::connect_stream(Arc::clone(&self.config), stream, client);
        let outcome = tokio::time::timeout(CONNECT_TIMEOUT, handshake).await;
        let mut handle = match outcome {
            Ok(Ok(handle)) => handle,
            Ok(Err(error)) => {
                return Err(rejected(&rejection).unwrap_or_else(|| self.failed(ssh, &error)));
            }
            Err(_) => {
                return Err(TunnelError::Connect {
                    host: ssh.host.clone(),
                    port: ssh.port,
                    reason: "SSH handshake timed out".into(),
                });
            }
        };
        authenticate(&mut handle, ssh).await?;
        Ok(handle)
    }

    fn failed(&self, ssh: &SshHost, error: &russh::Error) -> TunnelError {
        TunnelError::Connect {
            host: ssh.host.clone(),
            port: ssh.port,
            reason: error.to_string(),
        }
    }
}

fn rejected(rejection: &Rejection) -> Option<TunnelError> {
    rejection.lock().ok().and_then(|mut slot| slot.take())
}

/// Accepts until the tunnel closes or the SSH connection is lost. Dropping the `JoinSet` ends
/// every forwarded connection.
async fn accept_loop(
    listener: TcpListener,
    session: Arc<Handle<Client>>,
    target: (String, u16),
    mut lost: watch::Receiver<bool>,
) {
    let mut forwards = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((socket, peer)) = accepted else { continue };
                forwards.spawn(forward(socket, peer, Arc::clone(&session), target.clone()));
            }
            changed = lost.changed() => {
                if changed.is_err() || *lost.borrow() {
                    return;
                }
            }
            Some(_) = forwards.join_next(), if !forwards.is_empty() => {}
        }
    }
}

/// Carries one local socket to the target over a `direct-tcpip` channel. A refused channel
/// just closes the socket; the database driver reports it as a lost connection.
async fn forward(
    mut socket: TcpStream,
    peer: SocketAddr,
    session: Arc<Handle<Client>>,
    target: (String, u16),
) {
    let (host, port) = target;
    let channel = session
        .channel_open_direct_tcpip(
            host,
            u32::from(port),
            peer.ip().to_string(),
            u32::from(peer.port()),
        )
        .await;
    if let Ok(channel) = channel {
        let mut stream = channel.into_stream();
        // Either side closing ends the copy; there is nobody to report an I/O error to.
        let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
    }
}
