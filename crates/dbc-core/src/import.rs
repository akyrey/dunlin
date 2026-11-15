//! CSV import into a table (D-034): records are read on a blocking thread, turned into values
//! of each column's type, and inserted with multi-row `INSERT`s of bound values, all in one
//! transaction. Any failure rolls the whole import back.

mod csv;

use std::io::BufRead;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

pub use self::csv::{CsvRecords, Field, Record};
use crate::data::BoundSql;
use crate::dialect::Dialect;
use crate::driver::Connection;
use crate::error::DbError;
use crate::export::CsvOptions;
use crate::result::{ExecuteOptions, StatementOutcome, TypeCategory};
use crate::schema::{Column, TableRef};
use crate::value::Value;

/// Bound values per statement, under every supported database's limit (SQLite: 32766).
pub const MAX_PARAMETERS: usize = 30_000;
/// Rows per `INSERT`, however narrow the table.
pub const MAX_ROWS_PER_STATEMENT: usize = 500;
/// Records the preview shows.
pub const PREVIEW_ROWS: usize = 20;
/// Longest value quoted in an error message.
const QUOTED_VALUE_CHARS: usize = 40;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ImportError {
    #[error("Line {line}: {message}")]
    Csv { line: u64, message: String },
    #[error("Line {line}, column {column}: {message}")]
    Value {
        line: u64,
        column: String,
        message: String,
    },
    /// The database refused one `INSERT`: the rows of lines `first`..=`last`.
    #[error("Lines {first}–{last}: {error}")]
    Rows {
        first: u64,
        last: u64,
        error: DbError,
    },
    /// `BEGIN` or `COMMIT` failed, or the statement was cancelled between rows.
    #[error("{0}")]
    Database(DbError),
    #[error("{0}")]
    Io(String),
}

/// The first records of a file, for choosing the options and the column mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CsvPreview {
    /// The header row, or `Column 1`, `Column 2`, ... without one.
    pub headers: Vec<String>,
    /// `null` where the field reads as `NULL`.
    pub rows: Vec<Vec<Option<String>>>,
}

/// Where the import writes: each column with the index of the CSV field that fills it.
/// Columns left out take their default.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportTarget {
    pub table: TableRef,
    pub columns: Vec<(Column, usize)>,
}

fn quoted(text: &str) -> String {
    let short: String = text.chars().take(QUOTED_VALUE_CHARS).collect();
    let ellipsis = if short.len() < text.len() { "…" } else { "" };
    format!("\"{short}{ellipsis}\"")
}

fn hex_bytes(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            hex.get(i..i + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        })
        .collect()
}

/// A field as a value for `column`. An unquoted field equal to `null` is `NULL`; quoted fields
/// are always text. Numbers and booleans are checked here, so a bad value names its line;
/// other types (dates, JSON, UUIDs) are checked by the database.
pub fn field_value(field: &Field, column: &Column, null: &str) -> Result<Value, String> {
    if !field.quoted && field.text == null {
        return Ok(Value::Null);
    }
    let text = field.text.as_str();
    let trimmed = text.trim();
    match column.category {
        TypeCategory::Integer => trimmed
            .parse::<i64>()
            .map(Value::Int)
            .or_else(|_| trimmed.parse::<u64>().map(Value::UInt))
            .map_err(|_| format!("{} is not a whole number", quoted(text))),
        TypeCategory::Float => trimmed
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| format!("{} is not a number", quoted(text))),
        TypeCategory::Decimal => match trimmed.parse::<f64>() {
            Ok(n) if n.is_finite() => Ok(Value::Decimal(trimmed.to_owned())),
            _ => Err(format!("{} is not a number", quoted(text))),
        },
        TypeCategory::Bool => match trimmed.to_ascii_lowercase().as_str() {
            "true" | "t" | "yes" | "y" | "1" => Ok(Value::Bool(true)),
            "false" | "f" | "no" | "n" | "0" => Ok(Value::Bool(false)),
            _ => Err(format!("{} is not true or false", quoted(text))),
        },
        // `\x` hex, as the export writes binary; anything else is the text's own bytes.
        TypeCategory::Binary => match text.strip_prefix("\\x") {
            Some(hex) => hex_bytes(hex)
                .map(Value::Bytes)
                .ok_or_else(|| format!("{} is not \\x followed by hex digits", quoted(text))),
            None => Ok(Value::Bytes(text.as_bytes().to_vec())),
        },
        _ => Ok(Value::Text(field.text.clone())),
    }
}

/// The values of one record for the target's columns, in their order.
pub fn row_values(
    target: &ImportTarget,
    record: &Record,
    null: &str,
) -> Result<Vec<Value>, ImportError> {
    target
        .columns
        .iter()
        .map(|(column, source)| {
            let field = record.fields.get(*source).ok_or_else(|| ImportError::Csv {
                line: record.line,
                message: format!("no field {}", source + 1),
            })?;
            field_value(field, column, null).map_err(|message| ImportError::Value {
                line: record.line,
                column: column.name.clone(),
                message,
            })
        })
        .collect()
}

/// Rows per `INSERT` for a table of `columns` columns.
pub fn rows_per_statement(columns: usize) -> usize {
    (MAX_PARAMETERS / columns.max(1)).clamp(1, MAX_ROWS_PER_STATEMENT)
}

/// `INSERT INTO <table> (<columns>) VALUES (...), (...)` with every value bound.
pub fn insert_sql<D: Dialect + ?Sized>(
    dialect: &D,
    target: &ImportTarget,
    rows: &[Vec<Value>],
) -> BoundSql {
    let names: Vec<String> = target
        .columns
        .iter()
        .map(|(column, _)| dialect.quote_identifier(&column.name))
        .collect();
    let mut params = Vec::with_capacity(rows.len() * names.len());
    let tuples: Vec<String> = rows
        .iter()
        .map(|row| {
            let slots: Vec<String> = row
                .iter()
                .zip(&target.columns)
                .map(|(value, (column, _))| {
                    params.push(value.clone());
                    dialect.placeholder(params.len(), &column.type_name)
                })
                .collect();
            format!("({})", slots.join(", "))
        })
        .collect();
    BoundSql {
        sql: format!(
            "INSERT INTO {} ({}) VALUES {}",
            dialect.qualified_name(&target.table),
            names.join(", "),
            tuples.join(", ")
        ),
        params,
    }
}

/// Reads the file's records and hands them to `send` in batches of `batch` records. `needed`
/// is how many fields the mapping uses. Every record must have as many fields as the first.
/// Returns the header row when `options.header`; stops early when `send` returns `false`.
pub fn read_batches<R: BufRead>(
    reader: R,
    options: &CsvOptions,
    needed: usize,
    batch: usize,
    mut send: impl FnMut(Vec<Record>) -> bool,
) -> Result<Option<Vec<String>>, ImportError> {
    let mut records = CsvRecords::new(reader, options.delimiter);
    let mut width = None;
    let mut header = None;
    let mut pending = Vec::with_capacity(batch);
    while let Some(record) = records.next().transpose()? {
        let expected = *width.get_or_insert(record.fields.len());
        if expected < needed {
            let plural = if expected == 1 { "" } else { "s" };
            return Err(ImportError::Csv {
                line: record.line,
                message: format!(
                    "the file has {expected} column{plural}, but the mapping uses column {needed}"
                ),
            });
        }
        if record.fields.len() != expected {
            let found = record.fields.len();
            let plural = if found == 1 { "" } else { "s" };
            return Err(ImportError::Csv {
                line: record.line,
                message: format!("{found} field{plural} where the first row has {expected}"),
            });
        }
        if options.header && header.is_none() {
            header = Some(record.fields.into_iter().map(|f| f.text).collect());
            continue;
        }
        pending.push(record);
        if pending.len() >= batch && !send(std::mem::take(&mut pending)) {
            return Ok(header);
        }
    }
    if !pending.is_empty() {
        send(pending);
    }
    Ok(header)
}

/// The header (or generated names) and the first `rows` records, `NULL` fields as `None`.
pub fn preview<R: BufRead>(
    reader: R,
    options: &CsvOptions,
    rows: usize,
) -> Result<CsvPreview, ImportError> {
    let take = rows + usize::from(options.header);
    let records = CsvRecords::new(reader, options.delimiter)
        .take(take)
        .collect::<Result<Vec<_>, _>>()?;
    let mut records = records.into_iter();
    let named = if options.header {
        records
            .next()
            .map(|r| r.fields.into_iter().map(|f| f.text).collect())
    } else {
        None
    };
    let rows: Vec<Vec<Option<String>>> = records
        .map(|record| {
            record
                .fields
                .into_iter()
                .map(|f| (f.quoted || f.text != options.null).then_some(f.text))
                .collect()
        })
        .collect();
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let headers = named.unwrap_or_else(|| (1..=width).map(|i| format!("Column {i}")).collect());
    Ok(CsvPreview { headers, rows })
}

async fn control(connection: &dyn Connection, sql: &str) -> Result<(), ImportError> {
    connection
        .execute(sql, &ExecuteOptions::default())
        .await
        .map(|_| ())
        .map_err(ImportError::Database)
}

async fn insert_records(
    connection: &dyn Connection,
    dialect: &dyn Dialect,
    target: &ImportTarget,
    null: &str,
    records: &[Record],
) -> Result<u64, ImportError> {
    let mut inserted = 0;
    for chunk in records.chunks(rows_per_statement(target.columns.len())) {
        let rows = chunk
            .iter()
            .map(|record| row_values(target, record, null))
            .collect::<Result<Vec<_>, _>>()?;
        let bound = insert_sql(dialect, target, &rows);
        let outcome = connection
            .query_params(&bound.sql, &bound.params, &ExecuteOptions::default())
            .await;
        let lines = |error| ImportError::Rows {
            first: chunk.first().map_or(0, |r| r.line),
            last: chunk.last().map_or(0, |r| r.line),
            error,
        };
        match outcome {
            Ok(StatementOutcome::Affected { rows }) => inserted += rows,
            Ok(StatementOutcome::Rows(_)) => {
                inserted += u64::try_from(chunk.len()).unwrap_or(u64::MAX)
            }
            Err(error) => return Err(lines(error)),
        }
    }
    Ok(inserted)
}

/// Inserts the batches from `batches` in one transaction and returns the rows inserted.
/// `progress` hears the running total after each batch. A reader error arrives as `Err` and
/// rolls back like a database error.
pub async fn import(
    connection: &dyn Connection,
    dialect: &dyn Dialect,
    target: &ImportTarget,
    null: &str,
    mut batches: mpsc::Receiver<Result<Vec<Record>, ImportError>>,
    progress: &(dyn Fn(u64) + Send + Sync),
) -> Result<u64, ImportError> {
    control(connection, "BEGIN").await?;
    let mut total = 0;
    let mut outcome = Ok(());
    while let Some(batch) = batches.recv().await {
        let inserted = match batch {
            Ok(records) => insert_records(connection, dialect, target, null, &records).await,
            Err(error) => Err(error),
        };
        match inserted {
            Ok(rows) => {
                total += rows;
                progress(total);
            }
            Err(error) => {
                outcome = Err(error);
                break;
            }
        }
    }
    // Dropping the receiver stops the reader.
    drop(batches);
    if let Err(error) = outcome {
        rollback(connection).await;
        return Err(error);
    }
    if let Err(error) = control(connection, "COMMIT").await {
        rollback(connection).await;
        return Err(error);
    }
    Ok(total)
}

async fn rollback(connection: &dyn Connection) {
    // The error that caused the rollback is the one to report; a broken connection rolls back
    // on the server when it closes.
    let _ = connection
        .execute("ROLLBACK", &ExecuteOptions::default())
        .await;
}

#[cfg(test)]
mod tests;
