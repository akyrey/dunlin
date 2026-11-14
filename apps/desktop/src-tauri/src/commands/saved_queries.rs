//! Saved queries (task 8.6).

use dbc_store::{SavedQuery, SavedQueryDraft};

use crate::error::AppError;
use crate::ipc::{SavedQueryIdArgs, SavedQuerySaveArgs};
use crate::state::AppState;

pub async fn saved_query_list(state: &AppState) -> Result<Vec<SavedQuery>, AppError> {
    state
        .with_store(|store| store.list_saved_queries().map_err(AppError::from))
        .await
}

pub async fn saved_query_save(
    state: &AppState,
    args: SavedQuerySaveArgs,
) -> Result<SavedQuery, AppError> {
    let draft: SavedQueryDraft = args.query;
    state
        .with_store(move |store| {
            if let Some(connection) = &draft.connection_id {
                // A per-connection query needs a connection that exists.
                store.get_connection(connection)?;
            }
            store.save_query(draft).map_err(|e| match e {
                dbc_store::StoreError::NotFound(_) => AppError::not_found("The saved query"),
                other => AppError::from(other),
            })
        })
        .await
}

pub async fn saved_query_delete(state: &AppState, args: SavedQueryIdArgs) -> Result<(), AppError> {
    state
        .with_store(move |store| {
            store.delete_saved_query(&args.id).map_err(|e| match e {
                dbc_store::StoreError::NotFound(_) => AppError::not_found("The saved query"),
                other => AppError::from(other),
            })
        })
        .await
}
