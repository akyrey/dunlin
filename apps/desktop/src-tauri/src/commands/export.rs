//! Export (tasks 8.2, 8.3, 8.5): rows already shown, or a whole table streamed from the
//! database, written as CSV, JSON or SQL `INSERT`; and the same formats as text to copy.
//! A file is written under `<name>.partial` and renamed when complete, so a cancelled or
//! failed export leaves nothing that looks finished.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dbc_core::data::{self, KeyValue, TablePageRequest};
use dbc_core::dialect::Dialect;
use dbc_core::export::{ExportFormat, ExportWriter};
use dbc_core::result::{ColumnMeta, StreamEvent};
use dbc_core::schema::TableInfo;
use dbc_core::{DbError, Value};
use tokio::sync::mpsc;

use crate::error::AppError;
use crate::ipc::{ExportDone, ExportResultArgs, ExportTableArgs, ExportTextArgs, KeyArg};
use crate::state::AppState;

/// Batches in flight between the database and the file: memory stays flat.
const IN_FLIGHT: usize = 4;

type FileWriter = ExportWriter<BufWriter<File>>;

fn check_format(format: &ExportFormat) -> Result<(), AppError> {
    format
        .problem()
        .map_or(Ok(()), |problem| Err(AppError::invalid(problem)))
}

fn destination(path: &str) -> Result<(PathBuf, PathBuf), AppError> {
    let path = PathBuf::from(path);
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(AppError::invalid("Choose where to save the export"));
    }
    if !path.parent().is_some_and(Path::is_dir) {
        return Err(AppError::invalid(
            "The folder to save the export in does not exist",
        ));
    }
    let mut partial = path.clone().into_os_string();
    partial.push(".partial");
    Ok((path, PathBuf::from(partial)))
}

fn io_error(error: &std::io::Error) -> AppError {
    AppError::new("export", format!("Cannot write the export file: {error}"))
}

fn values(rows: Vec<Vec<dbc_core::WireValue>>) -> Result<Vec<Vec<Value>>, AppError> {
    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .map(|v| {
                    Value::try_from(v).map_err(|_| {
                        AppError::invalid(
                            "Some values were cut short for display; export the table to write them whole",
                        )
                    })
                })
                .collect()
        })
        .collect()
}

/// Renders rows in a format, for "copy as" (task 8.5).
pub async fn export_text(state: &AppState, args: ExportTextArgs) -> Result<String, AppError> {
    check_format(&args.format)?;
    let session = state.sessions.get(&args.session_id).await?;
    let rows = values(args.rows)?;
    let mut writer = ExportWriter::new(Vec::new(), args.format);
    writer.begin(&args.columns).map_err(AppError::from)?;
    writer
        .rows(&rows, session.dialect.as_ref())
        .map_err(AppError::from)?;
    let bytes = writer.finish().map_err(AppError::from)?;
    String::from_utf8(bytes).map_err(|_| AppError::internal("The export is not valid text"))
}

/// Writes rows the frontend already holds (a page, a query result) to a file (task 8.2).
pub async fn export_result(
    state: &AppState,
    args: ExportResultArgs,
) -> Result<ExportDone, AppError> {
    check_format(&args.format)?;
    let session = state.sessions.get(&args.session_id).await?;
    let (path, partial) = destination(&args.path)?;
    let rows = values(args.rows)?;
    let dialect = Arc::clone(&session.dialect);
    let columns = args.columns;
    let format = args.format;
    let target = partial.clone();
    let written = tokio::task::spawn_blocking(move || -> Result<u64, AppError> {
        let file = File::create(&target).map_err(|e| io_error(&e))?;
        let mut writer = ExportWriter::new(BufWriter::new(file), format);
        writer.begin(&columns).map_err(AppError::from)?;
        writer
            .rows(&rows, dialect.as_ref())
            .map_err(AppError::from)?;
        let count = writer.rows_written();
        writer.finish().map_err(AppError::from)?;
        Ok(count)
    })
    .await
    .map_err(|_| AppError::internal("The export task failed"))?;
    finish_file(written, &partial, &path)
}

fn finish_file(
    result: Result<u64, AppError>,
    partial: &Path,
    path: &Path,
) -> Result<ExportDone, AppError> {
    match result {
        Ok(rows) => {
            std::fs::rename(partial, path).map_err(|e| io_error(&e))?;
            Ok(ExportDone {
                rows_written: rows.to_string(),
                path: path.to_string_lossy().into_owned(),
            })
        }
        Err(error) => {
            // The partial file is useless; failing to remove it changes nothing for the user.
            let _ = std::fs::remove_file(partial);
            Err(error)
        }
    }
}

fn keys(key: Vec<KeyArg>) -> Result<Vec<KeyValue>, AppError> {
    key.into_iter()
        .map(|KeyArg { column, value }| match Value::try_from(value) {
            Ok(value) => Ok(KeyValue { column, value }),
            Err(e) => Err(AppError::invalid(format!("Key value for {column}: {e}"))),
        })
        .collect()
}

/// For `INSERT`, the columns an insert can write (no generated or identity-always columns),
/// with the table's own type names for the literals.
fn insert_columns(info: &TableInfo) -> Vec<String> {
    info.columns
        .iter()
        .filter(|c| !c.generated && c.default.as_deref() != Some("GENERATED ALWAYS AS IDENTITY"))
        .map(|c| c.name.clone())
        .collect()
}

fn with_table_types(columns: Vec<ColumnMeta>, info: &TableInfo) -> Vec<ColumnMeta> {
    columns
        .into_iter()
        .map(
            |column| match info.columns.iter().find(|c| c.name == column.name) {
                Some(described) => ColumnMeta {
                    type_name: described.type_name.clone(),
                    ..column
                },
                None => column,
            },
        )
        .collect()
}

/// Writes batches as they arrive, on a blocking thread, reporting progress after each.
async fn consume(
    mut rows: mpsc::Receiver<StreamEvent>,
    mut writer: FileWriter,
    info: &TableInfo,
    dialect: Arc<dyn Dialect>,
    progress: &(dyn Fn(u64) + Send + Sync),
) -> Result<u64, AppError> {
    while let Some(event) = rows.recv().await {
        writer = match event {
            StreamEvent::Columns(columns) => {
                let columns = with_table_types(columns, info);
                let mut w = writer;
                w.begin(&columns).map_err(AppError::from)?;
                w
            }
            StreamEvent::Rows(batch) => {
                let dialect = Arc::clone(&dialect);
                let w = tokio::task::spawn_blocking(move || -> Result<FileWriter, DbError> {
                    let mut w = writer;
                    w.rows(&batch, dialect.as_ref())?;
                    Ok(w)
                })
                .await
                .map_err(|_| AppError::internal("The export task failed"))?
                .map_err(AppError::from)?;
                progress(w.rows_written());
                w
            }
        };
    }
    let count = writer.rows_written();
    tokio::task::spawn_blocking(move || writer.finish().map(|_| ()))
        .await
        .map_err(|_| AppError::internal("The export task failed"))?
        .map_err(AppError::from)?;
    Ok(count)
}

/// Streams a whole table (with the view's filter and sort) to a file without holding it in
/// memory (task 8.3). Runs on the view's connection `viewId`: `query_cancel` stops it.
pub async fn export_table(
    state: &AppState,
    args: ExportTableArgs,
    progress: impl Fn(u64) + Send + Sync,
) -> Result<ExportDone, AppError> {
    check_format(&args.format)?;
    let session = state.sessions.get(&args.session_id).await?;
    let (path, partial) = destination(&args.path)?;
    let info = session
        .schema_table(&args.table)
        .await
        .map_err(AppError::from)?;
    let request = TablePageRequest {
        table: args.table,
        filter: args.filter,
        key: keys(args.key)?,
        sort: args.sort,
        limit: 0,
        offset: 0,
        after: Vec::new(),
    };
    let columns = matches!(args.format, ExportFormat::Insert { .. }).then(|| insert_columns(&info));
    let bound = data::select_sql(
        session.dialect.as_ref(),
        &info,
        &request,
        columns.as_deref(),
    )
    .map_err(AppError::from)?;
    let file = File::create(&partial).map_err(|e| io_error(&e))?;
    let writer = ExportWriter::new(BufWriter::new(file), args.format);
    let (tx, rx) = mpsc::channel(IN_FLIGHT);
    let producer = session.on_tab(&args.view_id, |connection| async move {
        connection.stream_rows(&bound.sql, &bound.params, tx).await
    });
    let consumer = consume(rx, writer, &info, Arc::clone(&session.dialect), &progress);
    let (produced, consumed) = tokio::join!(producer, consumer);
    // A failing writer stops reading, which stops the statement; the statement's own error
    // (a cancel, a lost connection) explains more than "the receiver went away".
    let result = match (produced, consumed) {
        (Err(error), _) => Err(AppError::from(error)),
        (Ok(_), result) => result,
    };
    finish_file(result, &partial, &path)
}
