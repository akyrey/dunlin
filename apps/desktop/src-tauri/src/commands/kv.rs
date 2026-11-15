//! The key browser of key-value connections (Redis, D-032).

use dbc_core::DEFAULT_DISPLAY_LIMIT;
use dbc_core::kv::{KeyName, ScanPage};

use crate::error::AppError;
use crate::ipc::{KeyContentIpc, KvEditArgs, KvReadArgs, KvScanArgs};
use crate::state::AppState;

/// Keys per `SCAN` step unless asked otherwise; a hint to the server, not a promise.
pub const DEFAULT_SCAN_COUNT: usize = 200;
/// Elements of a collection shown at most.
pub const MAX_KEY_ELEMENTS: usize = 10_000;
pub const DEFAULT_KEY_ELEMENTS: usize = 1_000;

pub async fn kv_scan(state: &AppState, args: KvScanArgs) -> Result<ScanPage, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let pattern = args
        .pattern
        .as_deref()
        .filter(|p| !p.is_empty())
        .unwrap_or("*");
    let count = args.count.unwrap_or(DEFAULT_SCAN_COUNT).clamp(1, 10_000);
    session
        .kv_scan(pattern, args.cursor.as_deref(), count)
        .await
        .map_err(AppError::from)
}

/// The commands an edit runs; with `preview: false` they run too. Read-only connections can
/// preview but not run.
pub async fn kv_edit(state: &AppState, args: KvEditArgs) -> Result<Vec<String>, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    if !args.preview && session.read_only {
        return Err(AppError::read_only());
    }
    let key = KeyName::decode(&args.key).map_err(|e| AppError::invalid(e.message))?;
    session
        .kv_edit(&key, &args.edit, args.preview)
        .await
        .map_err(|e| match e.code {
            dbc_core::DbErrorCode::Internal => AppError::invalid(e.message),
            _ => AppError::from(e),
        })
}

pub async fn kv_read(state: &AppState, args: KvReadArgs) -> Result<KeyContentIpc, AppError> {
    let session = state.sessions.get(&args.session_id).await?;
    let key = KeyName::decode(&args.key).map_err(|e| AppError::invalid(e.message))?;
    let limit = args
        .limit
        .unwrap_or(DEFAULT_KEY_ELEMENTS)
        .clamp(1, MAX_KEY_ELEMENTS);
    let content = session.kv_read(&key, limit).await.map_err(AppError::from)?;
    Ok(KeyContentIpc {
        key: content.key,
        key_type: content.key_type,
        ttl_ms: content.ttl_ms,
        length: content.length,
        columns: content.rows.columns,
        rows: content
            .rows
            .rows
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|v| v.into_wire(DEFAULT_DISPLAY_LIMIT))
                    .collect()
            })
            .collect(),
        truncated: content.rows.truncated,
    })
}
