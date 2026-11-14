//! The russh client handler: host key verification and loss detection.

use std::sync::{Arc, Mutex};

use russh::client::{DisconnectReason, Handler};
use russh::keys::PublicKeyOrCertificate;
use tokio::sync::watch;

use crate::error::TunnelError;
use crate::host_keys::{HostKeyStatus, HostKeyStore, algorithm, fingerprint};

/// Why the handler rejected the server, read back after `connect` fails.
pub(crate) type Rejection = Arc<Mutex<Option<TunnelError>>>;

pub(crate) struct Client {
    pub host: String,
    pub port: u16,
    pub store: Arc<dyn HostKeyStore>,
    pub rejection: Rejection,
    pub lost: Arc<watch::Sender<bool>>,
}

impl Client {
    fn reject(&self, error: TunnelError) {
        if let Ok(mut slot) = self.rejection.lock() {
            *slot = Some(error);
        }
    }
}

impl Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        presented: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = presented else {
            self.reject(TunnelError::Internal(
                "SSH host certificates are not supported".into(),
            ));
            return Ok(false);
        };
        let (host, port) = (self.host.clone(), self.port);
        let details = || (algorithm(key), fingerprint(key));
        match self.store.check(&host, port, key) {
            HostKeyStatus::Trusted => Ok(true),
            HostKeyStatus::Unknown => {
                let (algorithm, fingerprint) = details();
                self.reject(TunnelError::HostKeyUnknown {
                    host,
                    port,
                    algorithm,
                    fingerprint,
                });
                Ok(false)
            }
            HostKeyStatus::Changed => {
                let (algorithm, fingerprint) = details();
                self.reject(TunnelError::HostKeyChanged {
                    host,
                    port,
                    algorithm,
                    fingerprint,
                });
                Ok(false)
            }
        }
    }

    async fn disconnected(
        &mut self,
        reason: DisconnectReason<Self::Error>,
    ) -> Result<(), Self::Error> {
        // Receivers may all be gone after the tunnel closed; then nobody needs the signal.
        let _ = self.lost.send(true);
        match reason {
            DisconnectReason::ReceivedDisconnect(_) => Ok(()),
            DisconnectReason::Error(error) => Err(error),
        }
    }
}
