//! SSH user authentication: password, key file (optionally encrypted), or the agent.

use std::sync::Arc;

use russh::client::Handle;
use russh::keys::PrivateKeyWithHashAlg;
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::AgentClient;
use secrecy::ExposeSecret;

use crate::config::{SshAuth, SshHost};
use crate::error::TunnelError;
use crate::handler::Client;

pub(crate) async fn authenticate(
    handle: &mut Handle<Client>,
    ssh: &SshHost,
) -> Result<(), TunnelError> {
    let fail = |reason: String| TunnelError::Auth {
        host: ssh.host.clone(),
        user: ssh.user.clone(),
        reason,
    };
    let accepted = match &ssh.auth {
        SshAuth::Password(password) => handle
            .authenticate_password(ssh.user.clone(), password.expose_secret())
            .await
            .map_err(|e| fail(e.to_string()))?
            .success(),
        SshAuth::KeyFile { path, passphrase } => {
            let key =
                russh::keys::load_secret_key(path, passphrase.as_ref().map(|p| p.expose_secret()))
                    .map_err(|e| fail(key_error(&e, &path.display().to_string())))?;
            let hash = handle
                .best_supported_rsa_hash()
                .await
                .map_err(|e| fail(e.to_string()))?
                .flatten();
            handle
                .authenticate_publickey(
                    ssh.user.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await
                .map_err(|e| fail(e.to_string()))?
                .success()
        }
        SshAuth::Agent { socket } => with_agent(handle, ssh, socket).await.map_err(fail)?,
    };
    if accepted {
        Ok(())
    } else {
        Err(fail("the server rejected the credentials".into()))
    }
}

/// Tries each key the agent holds until one is accepted.
async fn with_agent(
    handle: &mut Handle<Client>,
    ssh: &SshHost,
    socket: &std::path::Path,
) -> Result<bool, String> {
    let mut agent = AgentClient::connect_uds(socket)
        .await
        .map_err(|e| format!("cannot reach the SSH agent at {}: {e}", socket.display()))?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|e| e.to_string())?;
    if identities.is_empty() {
        return Err("the SSH agent holds no keys".into());
    }
    let hash = handle
        .best_supported_rsa_hash()
        .await
        .map_err(|e| e.to_string())?
        .flatten();
    for identity in identities {
        let AgentIdentity::PublicKey { key, .. } = identity else {
            continue;
        };
        let result = handle
            .authenticate_publickey_with(ssh.user.clone(), key, hash, &mut agent)
            .await;
        if result.is_ok_and(|r| r.success()) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn key_error(error: &russh::keys::Error, path: &str) -> String {
    match error {
        russh::keys::Error::KeyIsEncrypted => format!("{path} is encrypted; enter its passphrase"),
        russh::keys::Error::IO(io) => format!("cannot read {path}: {io}"),
        other => format!("cannot use {path}: {other}"),
    }
}
