use std::time::Instant;

use dbc_core::driver::DriverInfo;
use dbc_store::ConnectionProfile;

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

pub async fn connection_save(
    state: &AppState,
    args: ConnectionSaveArgs,
) -> Result<ConnectionProfile, AppError> {
    let info = state.registry.get(args.profile.driver)?.info();
    profile::validate(&args.profile, &info)?;
    state
        .with_store(move |store| Ok(store.save_connection(args.profile)?))
        .await
}

pub async fn connection_delete(state: &AppState, args: ConnectionIdArgs) -> Result<(), AppError> {
    state
        .sessions
        .close_for_connection(&args.connection_id)
        .await?;
    state
        .with_store(move |store| Ok(store.delete_connection(&args.connection_id)?))
        .await
}

/// Connects with an unsaved profile, reads the server version and disconnects.
pub async fn connection_test(
    state: &AppState,
    args: ConnectionTestArgs,
) -> Result<ConnectionTestResult, AppError> {
    let driver = state.registry.get(args.profile.driver)?;
    let config = profile::config(&args.profile, &driver.info())?;
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
