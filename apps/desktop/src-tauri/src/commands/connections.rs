use std::time::Instant;

use dbc_core::driver::DriverInfo;
use dbc_store::{ConnectionProfile, SecretKind};
use secrecy::SecretString;

use crate::connect::{self, connect_error};
use crate::error::AppError;
use crate::ipc::{ConnectionIdArgs, ConnectionSaveArgs, ConnectionTestArgs, ConnectionTestResult};
use crate::profile;
use crate::state::AppState;

pub fn driver_list(state: &AppState) -> Vec<DriverInfo> {
    state.registry.list()
}

pub async fn connection_list(state: &AppState) -> Result<Vec<ConnectionProfile>, AppError> {
    state
        .with_store(|store| Ok(store.list_connections()?))
        .await
}

/// Saves the profile, then applies each secret: absent keeps, `null` deletes, a value replaces.
pub async fn connection_save(
    state: &AppState,
    args: ConnectionSaveArgs,
) -> Result<ConnectionProfile, AppError> {
    let info = state.registry.get(args.profile.driver)?.info();
    profile::validate(&args.profile, &info)?;
    let profile = args.profile;
    let saved = state
        .with_store(move |store| Ok(store.save_connection(profile)?))
        .await?;
    let changes: Vec<(SecretKind, Option<String>)> = args
        .secrets
        .unwrap_or_default()
        .changes()
        .into_iter()
        .filter_map(|(kind, change)| change.clone().map(|value| (kind, value)))
        .collect();
    if changes.is_empty() {
        return Ok(saved);
    }
    apply_secrets(state, &saved.id, changes).await
}

async fn apply_secrets(
    state: &AppState,
    id: &str,
    changes: Vec<(SecretKind, Option<String>)>,
) -> Result<ConnectionProfile, AppError> {
    let owned = id.to_owned();
    let flags: Vec<(SecretKind, bool)> = changes.iter().map(|(k, v)| (*k, v.is_some())).collect();
    state
        .with_secrets(move |secrets| {
            for (kind, value) in changes {
                match value {
                    Some(value) => secrets.set(&owned, kind, &SecretString::from(value))?,
                    None => secrets.delete(&owned, kind)?,
                }
            }
            Ok(())
        })
        .await?;
    let owned = id.to_owned();
    state
        .with_store(move |store| {
            for (kind, saved) in flags {
                store.set_secret_saved(&owned, kind, saved)?;
            }
            Ok(store.get_connection(&owned)?)
        })
        .await
}

/// Closes its sessions, removes the profile, then removes its keychain entries.
pub async fn connection_delete(state: &AppState, args: ConnectionIdArgs) -> Result<(), AppError> {
    state
        .sessions
        .close_for_connection(&args.connection_id)
        .await?;
    let id = args.connection_id.clone();
    state
        .with_store(move |store| Ok(store.delete_connection(&id)?))
        .await?;
    let id = args.connection_id;
    state
        .with_secrets(move |secrets| Ok(secrets.delete_all(&id)?))
        .await
}

/// Opens the tunnel (if any) and a database connection with an unsaved profile, reads the
/// server version, then closes both. Errors carry the failing stage in `detail.stage`.
pub async fn connection_test(
    state: &AppState,
    args: ConnectionTestArgs,
) -> Result<ConnectionTestResult, AppError> {
    let driver = state.registry.get(args.profile.driver)?;
    let saved = saved_secret_kinds(state, args.profile.id.as_deref()).await?;
    let secrets = connect::resolve_secrets(
        state,
        args.profile.id.as_deref(),
        &saved,
        &args.secrets.unwrap_or_default(),
    )
    .await?;
    let started = Instant::now();
    let prepared = connect::prepare(state, &args.profile, &driver.info(), &secrets).await?;
    let result = async {
        let connection = driver
            .connect(&prepared.config)
            .await
            .map_err(connect_error)?;
        let version = connection.server_version().await;
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        connection.close().await?;
        Ok(ConnectionTestResult {
            server_version: version?,
            latency_ms,
        })
    }
    .await;
    connect::discard(prepared.tunnel).await;
    result
}

/// Which secrets the keychain holds for a saved profile; none for an unsaved one.
pub(crate) async fn saved_secret_kinds(
    state: &AppState,
    id: Option<&str>,
) -> Result<Vec<SecretKind>, AppError> {
    let Some(id) = id.map(str::to_owned) else {
        return Ok(Vec::new());
    };
    state
        .with_store(move |store| match store.get_connection(&id) {
            Ok(profile) => Ok(profile.saved_secrets),
            Err(dbc_store::StoreError::NotFound(_)) => Ok(Vec::new()),
            Err(other) => Err(other.into()),
        })
        .await
}
