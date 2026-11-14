//! SSH tunnels for sessions: configuration from a profile, host key trust, error mapping.

use std::sync::Arc;

use dbc_store::{SecretKind, SshAuthMethod, SshHostSettings, SshSettings};
use dbc_tunnel::{
    CompositeHostKeyStore, DEFAULT_KEEPALIVE, FingerprintList, HostKeyStore, KnownHostsFile,
    SshAuth, SshHost, TrustedKey, TunnelConfig, TunnelError,
};
use serde_json::json;

use crate::connect::Secrets;
use crate::error::AppError;
use crate::state::AppState;

/// Which step of opening a connection failed (task 3.12), sent as `detail.stage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    SshConnect,
    HostKey,
    SshAuth,
    Forward,
    DbConnect,
    DbAuth,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SshConnect => "ssh_connect",
            Self::HostKey => "host_key",
            Self::SshAuth => "ssh_auth",
            Self::Forward => "forward",
            Self::DbConnect => "db_connect",
            Self::DbAuth => "db_auth",
        }
    }
}

/// The tunnel for `ssh` to `target_host:target_port`, with secrets from `secrets`.
pub fn tunnel_config(
    ssh: &SshSettings,
    target_host: &str,
    target_port: u16,
    secrets: &Secrets,
) -> Result<TunnelConfig, AppError> {
    let server = hop(
        &ssh.server,
        secrets,
        SecretKind::SshPassword,
        SecretKind::SshKeyPassphrase,
    )?;
    let jump = ssh
        .jump
        .as_ref()
        .map(|jump| {
            hop(
                jump,
                secrets,
                SecretKind::JumpPassword,
                SecretKind::JumpKeyPassphrase,
            )
        })
        .transpose()?;
    Ok(TunnelConfig {
        ssh: server,
        jump,
        target_host: target_host.to_owned(),
        target_port,
        keepalive: DEFAULT_KEEPALIVE,
    })
}

fn hop(
    settings: &SshHostSettings,
    secrets: &Secrets,
    password: SecretKind,
    passphrase: SecretKind,
) -> Result<SshHost, AppError> {
    let auth = match settings.auth {
        SshAuthMethod::Agent => SshAuth::agent_from_env().ok_or_else(|| {
            stage_error(
                "ssh_auth",
                "No SSH agent is running (SSH_AUTH_SOCK is not set)",
                Stage::SshAuth,
            )
        })?,
        SshAuthMethod::Password => SshAuth::Password(secrets.get(password).ok_or_else(|| {
            stage_error(
                "ssh_auth",
                format!("Enter the SSH password for {}", settings.host),
                Stage::SshAuth,
            )
        })?),
        SshAuthMethod::KeyFile => SshAuth::KeyFile {
            path: settings.key_file.clone().unwrap_or_default().into(),
            passphrase: secrets.get(passphrase),
        },
    };
    Ok(SshHost {
        host: settings.host.clone(),
        port: settings.port,
        user: settings.user.clone(),
        auth,
    })
}

/// The user's `~/.ssh/known_hosts` (read-only) together with the keys trusted in the app.
pub async fn host_key_store(state: &AppState) -> Result<Arc<dyn HostKeyStore>, AppError> {
    let trusted = state
        .with_store(|store| Ok(store.list_known_hosts()?))
        .await?;
    let app = FingerprintList {
        entries: trusted
            .into_iter()
            .map(|k| TrustedKey {
                host: k.host,
                port: k.port,
                algorithm: k.algorithm,
                fingerprint: k.fingerprint,
            })
            .collect(),
    };
    let mut stores: Vec<Box<dyn HostKeyStore>> = vec![Box::new(app)];
    if let Some(file) = KnownHostsFile::user_default() {
        stores.push(Box::new(file));
    }
    Ok(Arc::new(CompositeHostKeyStore::new(stores)))
}

pub fn stage_error(code: &str, message: impl Into<String>, stage: Stage) -> AppError {
    AppError::new(code, message).with_detail(json!({ "stage": stage.as_str() }))
}

impl From<TunnelError> for AppError {
    fn from(error: TunnelError) -> Self {
        let message = error.to_string();
        match error {
            TunnelError::Connect { .. } | TunnelError::Internal(_) => {
                stage_error("ssh_connect", message, Stage::SshConnect)
            }
            TunnelError::HostKeyUnknown { host, port, algorithm, fingerprint } => AppError::new("ssh_host_key_unknown", message)
                .with_detail(json!({ "stage": Stage::HostKey.as_str(), "host": host, "port": port, "algorithm": algorithm, "fingerprint": fingerprint })),
            TunnelError::HostKeyChanged { host, port, algorithm, fingerprint } => AppError::new("ssh_host_key_changed", message)
                .with_detail(json!({ "stage": Stage::HostKey.as_str(), "host": host, "port": port, "algorithm": algorithm, "fingerprint": fingerprint })),
            TunnelError::Auth { .. } => stage_error("ssh_auth", message, Stage::SshAuth),
            TunnelError::ForwardRefused { .. } => stage_error("ssh_forward", message, Stage::Forward),
            TunnelError::Lost => AppError::new("tunnel_lost", message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(auth: SshAuthMethod) -> SshHostSettings {
        SshHostSettings {
            host: "bastion".into(),
            port: 22,
            user: "me".into(),
            auth,
            key_file: Some("/k".into()),
        }
    }

    #[test]
    fn password_auth_needs_a_password() {
        let ssh = SshSettings {
            server: settings(SshAuthMethod::Password),
            jump: None,
        };

        let error = tunnel_config(&ssh, "db", 5432, &Secrets::default())
            .err()
            .unwrap();

        assert_eq!(error.code, "ssh_auth");
        assert_eq!(error.detail, Some(json!({ "stage": "ssh_auth" })));
    }

    #[test]
    fn key_files_carry_their_passphrase_and_jump_hosts_use_their_own_secrets() {
        let ssh = SshSettings {
            server: settings(SshAuthMethod::KeyFile),
            jump: Some(settings(SshAuthMethod::Password)),
        };
        let secrets = Secrets::from_pairs([
            (SecretKind::SshKeyPassphrase, "phrase"),
            (SecretKind::JumpPassword, "jump-pw"),
        ]);

        let config = tunnel_config(&ssh, "db.internal", 5432, &secrets).unwrap();

        assert!(matches!(
            &config.ssh.auth,
            SshAuth::KeyFile {
                passphrase: Some(_),
                ..
            }
        ));
        assert!(matches!(
            config.jump.map(|j| j.auth),
            Some(SshAuth::Password(_))
        ));
        assert_eq!(
            (config.target_host.as_str(), config.target_port),
            ("db.internal", 5432)
        );
    }

    #[test]
    fn tunnel_errors_carry_codes_and_stages() {
        let unknown = AppError::from(TunnelError::HostKeyUnknown {
            host: "b".into(),
            port: 22,
            algorithm: "ssh-ed25519".into(),
            fingerprint: "SHA256:x".into(),
        });
        let forward = AppError::from(TunnelError::ForwardRefused {
            target: "t".into(),
            reason: "r".into(),
        });
        let lost = AppError::from(TunnelError::Lost);

        assert_eq!(unknown.code, "ssh_host_key_unknown");
        assert_eq!(
            unknown.detail.as_ref().and_then(|d| d.get("fingerprint")),
            Some(&json!("SHA256:x"))
        );
        assert_eq!(forward.detail, Some(json!({ "stage": "forward" })));
        assert_eq!(lost.code, "tunnel_lost");
    }
}
