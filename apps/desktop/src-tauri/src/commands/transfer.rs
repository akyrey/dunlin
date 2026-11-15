//! Connection profiles to and from a file (deferred feature 7), for another machine or a
//! colleague. The file holds settings only: passwords and passphrases stay in the keychain.

use std::path::PathBuf;

use dbc_store::{ConnectionDraft, ConnectionProfile};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::ipc::{ConnectionsExportArgs, ConnectionsImportArgs, ImportReport};
use crate::profile;
use crate::state::AppState;

/// Marks the file as Dunlin's, so another JSON file is refused with a clear message.
pub const FORMAT: &str = "dunlin-connections";
pub const VERSION: u32 = 1;
/// A connections file is a few kilobytes; anything far larger is not one.
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
struct ConnectionsFile {
    format: String,
    version: u32,
    connections: Vec<ConnectionDraft>,
}

fn file_path(path: &str) -> Result<PathBuf, AppError> {
    let path = PathBuf::from(path);
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(AppError::invalid("Choose a file"));
    }
    Ok(path)
}

/// Writes the chosen connections (all when `ids` is absent), in list order. Returns how many.
pub async fn connections_export(
    state: &AppState,
    args: ConnectionsExportArgs,
) -> Result<usize, AppError> {
    let path = file_path(&args.path)?;
    let profiles = state
        .with_store(|store| Ok(store.list_connections()?))
        .await?;
    let connections: Vec<ConnectionDraft> = profiles
        .into_iter()
        .filter(|p| args.ids.as_ref().is_none_or(|ids| ids.contains(&p.id)))
        .map(|p| ConnectionDraft {
            id: None,
            ..ConnectionDraft::from(p)
        })
        .collect();
    let count = connections.len();
    let file = ConnectionsFile {
        format: FORMAT.to_owned(),
        version: VERSION,
        connections,
    };
    let text =
        serde_json::to_string_pretty(&file).map_err(|e| AppError::internal(e.to_string()))?;
    tokio::task::spawn_blocking(move || std::fs::write(&path, text))
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .map_err(|e| AppError::new("io", format!("The file could not be written: {e}")))?;
    Ok(count)
}

/// Adds every connection of the file as a new one. Entries that do not validate (unknown
/// driver, missing host, ...) are skipped and reported; the others are kept.
pub async fn connections_import(
    state: &AppState,
    args: ConnectionsImportArgs,
) -> Result<ImportReport, AppError> {
    let path = file_path(&args.path)?;
    let text = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
        if std::fs::metadata(&path)?.len() > MAX_FILE_BYTES {
            return Err(std::io::Error::other("the file is too large"));
        }
        std::fs::read_to_string(&path)
    })
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .map_err(|e| AppError::new("io", format!("The file could not be read: {e}")))?;
    let file: ConnectionsFile = serde_json::from_str(&text)
        .map_err(|e| AppError::invalid(format!("This is not a Dunlin connections file: {e}")))?;
    if file.format != FORMAT {
        return Err(AppError::invalid("This is not a Dunlin connections file"));
    }
    if file.version > VERSION {
        return Err(AppError::invalid(
            "The file comes from a newer Dunlin; update to import it",
        ));
    }
    let mut imported = Vec::new();
    let mut skipped = Vec::new();
    for draft in file.connections {
        match import_one(state, draft.clone()).await {
            Ok(profile) => imported.push(profile),
            Err(error) => skipped.push(format!("{}: {}", draft.name, error.message)),
        }
    }
    Ok(ImportReport { imported, skipped })
}

async fn import_one(
    state: &AppState,
    draft: ConnectionDraft,
) -> Result<ConnectionProfile, AppError> {
    let draft = ConnectionDraft { id: None, ..draft };
    let info = state.registry.get(draft.driver)?.info();
    profile::validate(&draft, &info)?;
    state
        .with_store(move |store| Ok(store.save_connection(draft)?))
        .await
}
