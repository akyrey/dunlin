use dbc_store::KnownHost;

use crate::error::AppError;
use crate::ipc::{SshKnownHostRemoveArgs, SshTrustHostArgs};
use crate::state::AppState;

/// Stores a host key the user accepted after seeing its fingerprint. Never replaces a
/// different key for the same host (SECURITY.md#ssh).
pub async fn ssh_trust_host(state: &AppState, args: SshTrustHostArgs) -> Result<(), AppError> {
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
