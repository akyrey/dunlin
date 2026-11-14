//! From a profile to something connectable: secrets, then the tunnel, then the driver config.

use std::collections::HashMap;

use dbc_core::driver::{ConnectionConfig, DriverInfo, Endpoint};
use dbc_core::{DbError, DbErrorCode};
use dbc_store::{ConnectionDraft, SecretKind};
use dbc_tunnel::Tunnel;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;

use crate::error::AppError;
use crate::ipc::ConnectionSecrets;
use crate::profile;
use crate::ssh::{self, Stage};
use crate::state::AppState;

/// The secrets for one connection attempt. Never serialised, never logged.
#[derive(Default)]
pub struct Secrets {
    values: HashMap<SecretKind, SecretString>,
}

impl Secrets {
    pub fn get(&self, kind: SecretKind) -> Option<SecretString> {
        self.values
            .get(&kind)
            .map(|s| SecretString::from(s.expose_secret().to_owned()))
    }

    #[cfg(test)]
    pub fn from_pairs<const N: usize>(pairs: [(SecretKind, &str); N]) -> Self {
        let values = pairs
            .into_iter()
            .map(|(k, v)| (k, SecretString::from(v.to_owned())))
            .collect();
        Self { values }
    }
}

/// Given secrets win; otherwise a saved profile's stored ones are read from the keychain.
pub async fn resolve_secrets(
    state: &AppState,
    id: Option<&str>,
    saved: &[SecretKind],
    given: &ConnectionSecrets,
) -> Result<Secrets, AppError> {
    let mut values = HashMap::new();
    let mut to_read = Vec::new();
    for (kind, change) in given.changes() {
        match change {
            Some(Some(value)) => {
                values.insert(kind, SecretString::from(value.clone()));
            }
            Some(None) => {}
            None if saved.contains(&kind) => to_read.push(kind),
            None => {}
        }
    }
    if let (Some(id), false) = (id, to_read.is_empty()) {
        let id = id.to_owned();
        let stored = state
            .with_secrets(move |store| {
                let mut found = Vec::new();
                for kind in to_read {
                    if let Some(secret) = store.get(&id, kind)? {
                        found.push((kind, secret));
                    }
                }
                Ok(found)
            })
            .await?;
        values.extend(stored);
    }
    Ok(Secrets { values })
}

/// A driver configuration, plus the tunnel it dials through (if any). The caller owns the
/// tunnel and must close it after the connections that use it.
pub struct Prepared {
    pub config: ConnectionConfig,
    pub tunnel: Option<Tunnel>,
}

pub async fn prepare(
    state: &AppState,
    draft: &ConnectionDraft,
    info: &DriverInfo,
    secrets: &Secrets,
) -> Result<Prepared, AppError> {
    let config = profile::config(draft, info, secrets.get(SecretKind::DbPassword))?;
    let Some(ssh_settings) = &draft.ssh else {
        return Ok(Prepared {
            config,
            tunnel: None,
        });
    };
    let Endpoint::Tcp {
        dial_host,
        dial_port,
        server_name,
    } = &config.endpoint
    else {
        return Err(AppError::invalid(
            "Only network databases can use an SSH tunnel",
        ));
    };
    let tunnel_config = ssh::tunnel_config(ssh_settings, dial_host, *dial_port, secrets)?;
    let tunnel = Tunnel::open(&tunnel_config, ssh::host_key_store(state).await?).await?;
    let local = tunnel.local_addr();
    // D-004: dial the loopback listener, verify TLS against the real server name.
    let endpoint = Endpoint::Tcp {
        dial_host: local.ip().to_string(),
        dial_port: local.port(),
        server_name: server_name.clone(),
    };
    Ok(Prepared {
        config: ConnectionConfig { endpoint, ..config },
        tunnel: Some(tunnel),
    })
}

/// A database error while connecting, labelled with the stage for "Test connection".
pub fn connect_error(error: DbError) -> AppError {
    let stage = if error.code == DbErrorCode::Auth {
        Stage::DbAuth
    } else {
        Stage::DbConnect
    };
    let app = AppError::from(error);
    let mut detail = app.detail.clone().unwrap_or_else(|| json!({}));
    if let Some(object) = detail.as_object_mut() {
        object.insert("stage".into(), json!(stage.as_str()));
    }
    app.with_detail(detail)
}

/// Closes a tunnel that is no longer needed. A tunnel that fails to close cleanly has already
/// stopped listening, which is what matters here.
pub async fn discard(tunnel: Option<Tunnel>) {
    if let Some(tunnel) = tunnel {
        let _ = tunnel.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_errors_are_labelled_with_their_stage() {
        let auth = connect_error(DbError::new(DbErrorCode::Auth, "bad password"));
        let refused =
            connect_error(DbError::new(DbErrorCode::Connect, "refused").with_sqlstate("08001"));

        assert_eq!(auth.detail, Some(json!({ "stage": "db_auth" })));
        assert_eq!(
            refused.detail,
            Some(json!({ "stage": "db_connect", "sqlstate": "08001" }))
        );
    }
}
