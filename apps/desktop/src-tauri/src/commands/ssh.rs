use dbc_store::KnownHost;

use crate::error::AppError;
use crate::ipc::{SshConfigEntry, SshKnownHostRemoveArgs, SshTrustHostArgs};
use crate::state::AppState;

/// Stores a host key the user accepted after seeing its fingerprint. Never replaces a
/// different key for the same host (SECURITY.md#ssh).
pub async fn ssh_trust_host(state: &AppState, args: SshTrustHostArgs) -> Result<(), AppError> {
    check_host_key(&args)?;
    let key = KnownHost {
        host: args.host,
        port: args.port,
        algorithm: args.algorithm,
        fingerprint: args.fingerprint,
    };
    state
        .with_store(move |store| Ok(store.trust_host(&key)?))
        .await
}

/// Longest host name DNS allows.
const MAX_HOST: usize = 253;
const MAX_ALGORITHM: usize = 64;
const MAX_FINGERPRINT: usize = 128;

/// The key must look like one the backend reported (`ssh_host_key_unknown`): a host, an
/// algorithm name and a SHA-256 fingerprint.
fn check_host_key(args: &SshTrustHostArgs) -> Result<(), AppError> {
    let host = args.host.trim();
    if host.is_empty() || host.len() > MAX_HOST || args.port == 0 {
        return Err(AppError::invalid("The SSH host or port is not valid"));
    }
    let algorithm_ok = !args.algorithm.is_empty()
        && args.algorithm.len() <= MAX_ALGORITHM
        && args.algorithm.bytes().all(|b| b.is_ascii_graphic());
    let fingerprint_ok = args.fingerprint.starts_with("SHA256:")
        && args.fingerprint.len() <= MAX_FINGERPRINT
        && args.fingerprint.bytes().all(|b| b.is_ascii_graphic());
    if !algorithm_ok || !fingerprint_ok {
        return Err(AppError::invalid("The SSH host key is not valid"));
    }
    Ok(())
}

/// The host aliases of `~/.ssh/config`, to fill a connection's SSH settings from. Read when
/// asked, never stored; a missing file has none.
pub async fn ssh_config_hosts() -> Result<Vec<SshConfigEntry>, AppError> {
    let hosts = tokio::task::spawn_blocking(dbc_tunnel::user_ssh_config)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .map_err(|e| AppError::new("io", format!("~/.ssh/config could not be read: {e}")))?;
    Ok(hosts.into_iter().map(SshConfigEntry::from).collect())
}

pub async fn ssh_known_hosts_list(state: &AppState) -> Result<Vec<KnownHost>, AppError> {
    state
        .with_store(|store| Ok(store.list_known_hosts()?))
        .await
}

pub async fn ssh_known_host_remove(
    state: &AppState,
    args: SshKnownHostRemoveArgs,
) -> Result<(), AppError> {
    state
        .with_store(move |store| {
            Ok(store.remove_known_host(&args.host, args.port, &args.algorithm)?)
        })
        .await
}
