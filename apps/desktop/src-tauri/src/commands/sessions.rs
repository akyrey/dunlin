use dbc_store::ConnectionDraft;

use crate::connect::{self, connect_error};
use crate::error::AppError;
use crate::ipc::{ConnectionIdArgs, ConnectionSecrets, SessionIdArgs, SessionInfo, TabArgs};
use crate::state::AppState;

/// Opens the tunnel (if any), then the session's metadata connection through it.
pub async fn session_open(
    state: &AppState,
    args: ConnectionIdArgs,
) -> Result<SessionInfo, AppError> {
    let id = args.connection_id.clone();
    let saved = state
        .with_store(move |store| Ok(store.get_connection(&id)?))
        .await?;
    let driver = state.registry.get(saved.driver)?;
    let secrets = connect::resolve_secrets(
        state,
        Some(&saved.id),
        &saved.saved_secrets,
        &ConnectionSecrets::default(),
    )
    .await?;
    let draft = ConnectionDraft::from(saved);
    let prepared = connect::prepare(state, &draft, &driver.info(), &secrets).await?;
    let session = state
        .sessions
        .open(
            &args.connection_id,
            driver,
            prepared.config,
            prepared.tunnel,
        )
        .await
        .map_err(connect_error)?;
    Ok(SessionInfo {
        session_id: session.id.clone(),
        connection_id: session.connection_id.clone(),
        driver: session.kind,
        capabilities: session.capabilities,
        read_only: session.read_only,
    })
}

pub async fn session_close(state: &AppState, args: SessionIdArgs) -> Result<(), AppError> {
    state.sessions.close(&args.session_id).await
}

pub async fn tab_close(state: &AppState, args: TabArgs) -> Result<(), AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    session
        .close_tab(&args.tab_id)
        .await
        .map_err(AppError::from)
}
