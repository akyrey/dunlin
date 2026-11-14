use dbc_store::ConnectionDraft;

use crate::error::AppError;
use crate::ipc::{ConnectionIdArgs, SessionIdArgs, SessionInfo, TabArgs};
use crate::profile;
use crate::state::AppState;

pub async fn session_open(
    state: &AppState,
    args: ConnectionIdArgs,
) -> Result<SessionInfo, AppError> {
    let id = args.connection_id.clone();
    let saved = state
        .with_store(move |store| Ok(store.get_connection(&id)?))
        .await?;
    let driver = state.registry.get(saved.driver)?;
    let config = profile::config(&ConnectionDraft::from(saved), &driver.info())?;
    let session = state
        .sessions
        .open(&args.connection_id, driver, config)
        .await?;
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
