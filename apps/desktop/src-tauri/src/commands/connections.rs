use std::time::Instant;

use dbc_core::driver::DriverInfo;
use dbc_store::{ConnectionDraft, ConnectionProfile, SecretKind};
use secrecy::SecretString;

use crate::error::AppError;
use crate::ipc::{
    ConnectionIdArgs, ConnectionSaveArgs, ConnectionSecrets, ConnectionTestArgs,
    ConnectionTestResult,
};
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

/// Saves the profile, then applies the secrets: absent keeps, `null` deletes, a value replaces.
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
    match args.secrets.unwrap_or_default().db_password {
        None => Ok(saved),
        Some(password) => set_db_password(state, &saved.id, password).await,
    }
}

async fn set_db_password(
    state: &AppState,
    id: &str,
    password: Option<String>,
) -> Result<ConnectionProfile, AppError> {
    let owned = id.to_owned();
    let stored = password.is_some();
    state
        .with_secrets(move |secrets| {
            match password {
                Some(value) => {
                    secrets.set(&owned, SecretKind::DbPassword, &SecretString::from(value))?
                }
                None => secrets.delete(&owned, SecretKind::DbPassword)?,
            }
            Ok(())
        })
        .await?;
    let owned = id.to_owned();
    state
        .with_store(move |store| {
            store.set_has_db_password(&owned, stored)?;
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

/// Connects with an unsaved profile, reads the server version and disconnects. The password
/// is the one given, else the stored one for a saved profile.
pub async fn connection_test(
    state: &AppState,
    args: ConnectionTestArgs,
) -> Result<ConnectionTestResult, AppError> {
    let driver = state.registry.get(args.profile.driver)?;
    let password = password_for_test(state, &args.profile, args.secrets).await?;
    let config = profile::config(&args.profile, &driver.info(), password)?;
    let started = Instant::now();
    let connection = driver.connect(&config).await?;
    let version = connection.server_version().await;
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    connection.close().await?;
    Ok(ConnectionTestResult {
        server_version: version?,
        latency_ms,
    })
}

async fn password_for_test(
    state: &AppState,
    draft: &ConnectionDraft,
    secrets: Option<ConnectionSecrets>,
) -> Result<Option<SecretString>, AppError> {
    match secrets.unwrap_or_default().db_password {
        Some(given) => Ok(given.map(SecretString::from)),
        None => match &draft.id {
            Some(id) => stored_password(state, id).await,
            None => Ok(None),
        },
    }
}

/// The keychain password for a saved connection, if one is stored.
pub(crate) async fn stored_password(
    state: &AppState,
    id: &str,
) -> Result<Option<SecretString>, AppError> {
    let owned = id.to_owned();
    state
        .with_secrets(move |secrets| Ok(secrets.get(&owned, SecretKind::DbPassword)?))
        .await
}
