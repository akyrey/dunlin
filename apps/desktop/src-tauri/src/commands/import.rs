//! CSV import (D-034): a preview of the file, then every record inserted in one transaction on
//! the import's own connection. The file is read on a blocking thread and handed over in
//! batches, so it is never held in memory whole.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use dbc_core::DbErrorCode;
use dbc_core::export::{CsvOptions, ExportFormat};
use dbc_core::import::{self, CsvPreview, ImportError, ImportTarget, PREVIEW_ROWS};
use dbc_core::schema::TableInfo;
use serde_json::json;
use tokio::sync::mpsc;

use crate::error::AppError;
use crate::ipc::{ImportColumn, ImportCsvArgs, ImportDone, ImportPreviewArgs};
use crate::state::AppState;

/// Records per batch handed from the reader to the inserts.
const READ_BATCH: usize = 1000;
/// Batches read ahead of the inserts.
const IN_FLIGHT: usize = 4;

fn check_options(options: &CsvOptions) -> Result<(), AppError> {
    ExportFormat::Csv(options.clone())
        .problem()
        .map_or(Ok(()), |problem| Err(AppError::invalid(problem)))
}

fn open(path: &str) -> Result<BufReader<File>, ImportError> {
    if !Path::new(path).is_absolute() {
        return Err(ImportError::Io("Choose a CSV file to import".into()));
    }
    File::open(path)
        .map(BufReader::new)
        .map_err(|e| ImportError::Io(format!("Cannot open the CSV file: {e}")))
}

fn app_error(error: ImportError) -> AppError {
    match error {
        ImportError::Csv { line, .. } | ImportError::Value { line, .. } => {
            AppError::invalid(error.to_string()).with_detail(json!({ "line": line }))
        }
        ImportError::Rows { first, last, error } => {
            let message = format!("Lines {first}–{last}: {}", error.message);
            let base = AppError::from(error);
            let mut detail = base.detail.clone().unwrap_or_else(|| json!({}));
            detail["firstLine"] = json!(first);
            detail["lastLine"] = json!(last);
            AppError::new(&base.code, message).with_detail(detail)
        }
        ImportError::Database(error) => AppError::from(error),
        ImportError::Io(message) => AppError::new("import", message),
    }
}

/// The header (or generated names) and the first records of the file.
pub async fn import_preview(args: ImportPreviewArgs) -> Result<CsvPreview, AppError> {
    check_options(&args.options)?;
    tokio::task::spawn_blocking(move || {
        import::preview(open(&args.path)?, &args.options, PREVIEW_ROWS)
    })
    .await
    .map_err(|_| AppError::internal("The preview task failed"))?
    .map_err(app_error)
}

/// The target columns in table order; each must exist, be writable and appear once.
fn target(info: &TableInfo, columns: &[ImportColumn]) -> Result<ImportTarget, AppError> {
    if columns.is_empty() {
        return Err(AppError::invalid("Choose at least one column to import"));
    }
    let mut mapped = Vec::new();
    for column in &info.columns {
        let sources: Vec<usize> = columns
            .iter()
            .filter(|c| c.column == column.name)
            .map(|c| c.source)
            .collect();
        match sources.as_slice() {
            [] => {}
            [source] if !column.generated => mapped.push((column.clone(), *source)),
            [_] => {
                return Err(AppError::invalid(format!(
                    "{} is generated and cannot be imported into",
                    column.name
                )));
            }
            _ => {
                return Err(AppError::invalid(format!(
                    "{} is mapped more than once",
                    column.name
                )));
            }
        }
    }
    if let Some(unknown) = columns
        .iter()
        .find(|c| !info.columns.iter().any(|t| t.name == c.column))
    {
        return Err(AppError::not_found(&format!(
            "The column {}",
            unknown.column
        )));
    }
    Ok(ImportTarget {
        table: info.table.clone(),
        columns: mapped,
    })
}

/// Reads the file into `tx` on a blocking thread; a read error is the last message.
fn spawn_reader(
    path: String,
    options: CsvOptions,
    needed: usize,
    tx: mpsc::Sender<Result<Vec<import::Record>, ImportError>>,
) {
    tokio::task::spawn_blocking(move || {
        let read = open(&path).and_then(|reader| {
            import::read_batches(reader, &options, needed, READ_BATCH, |batch| {
                tx.blocking_send(Ok(batch)).is_ok()
            })
        });
        if let Err(error) = read {
            // The import has already stopped when nobody receives the error.
            let _ = tx.blocking_send(Err(error));
        }
    });
}

/// Inserts the file's records; `progress` hears the running total after each batch.
pub async fn import_csv(
    state: &AppState,
    args: ImportCsvArgs,
    progress: impl Fn(u64) + Send + Sync,
) -> Result<ImportDone, AppError> {
    check_options(&args.options)?;
    let session = state.sessions.get(&args.session_id).await?;
    if session.read_only {
        return Err(AppError::read_only());
    }
    let info = session.schema_table(&args.table).await;
    let info = super::table_data::finish(state, &session, info).await?;
    let target = target(&info, &args.columns)?;
    let needed = target.columns.iter().map(|(_, s)| s + 1).max().unwrap_or(0);
    let (tx, rx) = mpsc::channel(IN_FLIGHT);
    spawn_reader(args.path, args.options.clone(), needed, tx);
    let dialect = std::sync::Arc::clone(&session.dialect);
    let null = args.options.null;
    let outcome = session
        .on_tab(&args.import_id, |connection| async move {
            let imported = import::import(
                connection.as_ref(),
                dialect.as_ref(),
                &target,
                &null,
                rx,
                &progress,
            )
            .await;
            match imported {
                // A lost connection is the session's business (reconnect, tunnel).
                Err(ImportError::Rows { error, .. } | ImportError::Database(error))
                    if error.code == DbErrorCode::ConnectionLost =>
                {
                    Err(error)
                }
                other => Ok(other),
            }
        })
        .await;
    let rows = super::table_data::finish(state, &session, outcome)
        .await?
        .map_err(app_error)?;
    Ok(ImportDone {
        rows_inserted: rows.to_string(),
    })
}
