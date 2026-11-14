//! Export writers (task 8.1): CSV, JSON and SQL `INSERT`, written a batch of rows at a time so
//! a whole table streams to a file without being held in memory.

use std::io::Write;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

use crate::dialect::{Dialect, hex};
use crate::error::{DbError, DbErrorCode};
use crate::result::ColumnMeta;
use crate::schema::TableRef;
use crate::value::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CsvOptions {
    /// One character: `,`, `;`, `\t` or `|`.
    pub delimiter: char,
    pub header: bool,
    /// Written for `NULL`; empty by default, which `COPY ... CSV` reads as `NULL`.
    pub null: String,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            delimiter: ',',
            header: true,
            null: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExportFormat {
    Csv(CsvOptions),
    Json,
    /// `INSERT INTO <table>` with up to `batch` rows per statement.
    Insert {
        table: TableRef,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        batch: usize,
    },
}

/// Rows per `INSERT` unless the format says otherwise.
pub const DEFAULT_INSERT_BATCH: usize = 100;

fn io(error: &std::io::Error) -> DbError {
    DbError::new(
        DbErrorCode::Internal,
        format!("Cannot write the export file: {error}"),
    )
}

/// The text of a value in CSV: what each database's own import reads back as the same value.
fn csv_text(value: &Value) -> Option<String> {
    Some(match value {
        Value::Null => return None,
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::UInt(u) => u.to_string(),
        Value::Float(f) => f.to_string(),
        // PostgreSQL's bytea text form; MySQL imports it with UNHEX on the column.
        Value::Bytes(bytes) => format!("\\x{}", hex(bytes)),
        Value::Decimal(s)
        | Value::Text(s)
        | Value::Json(s)
        | Value::Xml(s)
        | Value::Uuid(s)
        | Value::Date(s)
        | Value::Time(s)
        | Value::Timestamp(s)
        | Value::TimestampTz(s)
        | Value::Interval(s) => s.clone(),
        Value::Array(items) => format!(
            "{{{}}}",
            items
                .iter()
                .map(|v| csv_text(v).map_or_else(|| "NULL".to_owned(), |t| array_element(&t)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Value::Unknown { text: Some(t), .. } => t.clone(),
        Value::Unknown { hex: Some(h), .. } => format!("\\x{h}"),
        Value::Unknown { .. } => return None,
    })
}

/// A PostgreSQL array element, quoted when it needs to be.
fn array_element(text: &str) -> String {
    let plain = !text.is_empty()
        && !text.eq_ignore_ascii_case("null")
        && !text.contains(['"', '\\', ',', '{', '}', ' ']);
    if plain {
        text.to_owned()
    } else {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// RFC 4180: quoted when the field holds the delimiter, a quote or a line break. Text that
/// reads like `NULL` (empty, or the chosen `NULL` text) is quoted too, so importers (`COPY`,
/// `LOAD DATA`) tell it from `NULL`.
fn csv_field(text: &str, delimiter: char, null: &str) -> String {
    if text.is_empty() || text == null {
        return format!("\"{}\"", text.replace('"', "\"\""));
    }
    if text.contains([delimiter, '"', '\r', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

/// A JSON number when a JavaScript number holds the value exactly, else a string, as in the
/// JSON view (task 6.9).
fn exact_number(text: &str) -> Option<String> {
    let number: f64 = text.parse().ok()?;
    if !number.is_finite() {
        return None;
    }
    let canonical = |s: &str| {
        let s = s.trim_start_matches('+');
        if s.contains('.') && !s.contains(['e', 'E']) {
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            s.to_owned()
        }
    };
    let integer = text
        .bytes()
        .all(|b| b.is_ascii_digit() || b == b'-' || b == b'+');
    if integer {
        // 2^53: beyond it a double cannot hold every integer.
        const SAFE: f64 = 9_007_199_254_740_991.0;
        return (number.abs() <= SAFE).then(|| canonical(text));
    }
    (canonical(&number.to_string()) == canonical(text)).then(|| canonical(text))
}

fn json_string(text: &str) -> String {
    serde_json::Value::String(text.to_owned()).to_string()
}

fn json_value(value: &Value) -> String {
    match value {
        Value::Null
        | Value::Unknown {
            text: None,
            hex: None,
            ..
        } => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Int(_) | Value::UInt(_) | Value::Float(_) | Value::Decimal(_) => {
            let text = csv_text(value).unwrap_or_default();
            exact_number(&text).unwrap_or_else(|| json_string(&text))
        }
        Value::Bytes(bytes) => json_string(&BASE64.encode(bytes)),
        // A valid document is embedded as it is, so its own numbers keep every digit.
        Value::Json(text) if serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok() => {
            text.clone()
        }
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(json_value).collect::<Vec<_>>().join(",")
        ),
        other => json_string(&csv_text(other).unwrap_or_default()),
    }
}

/// Writes one export file a batch at a time: `begin`, any number of `rows`, then `finish`.
pub struct ExportWriter<W: Write> {
    out: W,
    format: ExportFormat,
    columns: Vec<ColumnMeta>,
    rows_written: u64,
}

impl<W: Write> ExportWriter<W> {
    pub fn new(out: W, format: ExportFormat) -> Self {
        Self {
            out,
            format,
            columns: Vec::new(),
            rows_written: 0,
        }
    }

    pub fn rows_written(&self) -> u64 {
        self.rows_written
    }

    pub fn begin(&mut self, columns: &[ColumnMeta]) -> Result<(), DbError> {
        self.columns = columns.to_vec();
        match &self.format {
            ExportFormat::Csv(options) if options.header => {
                let header: Vec<String> = columns
                    .iter()
                    .map(|c| csv_field(&c.name, options.delimiter, &options.null))
                    .collect();
                let line = header.join(&options.delimiter.to_string());
                write!(self.out, "{line}\r\n").map_err(|e| io(&e))
            }
            ExportFormat::Json => self.out.write_all(b"[").map_err(|e| io(&e)),
            _ => Ok(()),
        }
    }

    pub fn rows(&mut self, rows: &[Vec<Value>], dialect: &dyn Dialect) -> Result<(), DbError> {
        match self.format.clone() {
            ExportFormat::Csv(options) => self.csv(rows, &options),
            ExportFormat::Json => self.json(rows),
            ExportFormat::Insert { table, batch } => {
                self.insert(rows, &table, batch.max(1), dialect)
            }
        }?;
        self.rows_written += rows.len() as u64;
        Ok(())
    }

    fn csv(&mut self, rows: &[Vec<Value>], options: &CsvOptions) -> Result<(), DbError> {
        let delimiter = options.delimiter.to_string();
        for row in rows {
            let fields: Vec<String> = row
                .iter()
                .map(|v| {
                    csv_text(v).map_or_else(
                        || options.null.clone(),
                        |t| csv_field(&t, options.delimiter, &options.null),
                    )
                })
                .collect();
            write!(self.out, "{}\r\n", fields.join(&delimiter)).map_err(|e| io(&e))?;
        }
        Ok(())
    }

    fn json(&mut self, rows: &[Vec<Value>]) -> Result<(), DbError> {
        for (i, row) in rows.iter().enumerate() {
            let separator = if self.rows_written == 0 && i == 0 {
                "\n"
            } else {
                ",\n"
            };
            let fields: Vec<String> = self
                .columns
                .iter()
                .zip(row)
                .map(|(column, value)| {
                    format!("{}: {}", json_string(&column.name), json_value(value))
                })
                .collect();
            write!(self.out, "{separator}  {{{}}}", fields.join(", ")).map_err(|e| io(&e))?;
        }
        Ok(())
    }

    fn insert(
        &mut self,
        rows: &[Vec<Value>],
        table: &TableRef,
        batch: usize,
        dialect: &dyn Dialect,
    ) -> Result<(), DbError> {
        let columns: Vec<String> = self
            .columns
            .iter()
            .map(|c| dialect.quote_identifier(&c.name))
            .collect();
        let head = format!(
            "INSERT INTO {} ({}) VALUES",
            dialect.qualified_name(table),
            columns.join(", ")
        );
        for chunk in rows.chunks(batch) {
            let tuples: Vec<String> = chunk
                .iter()
                .map(|row| {
                    let literals: Vec<String> = row
                        .iter()
                        .zip(&self.columns)
                        .map(|(value, column)| dialect.render_literal(value, &column.type_name))
                        .collect();
                    format!("({})", literals.join(", "))
                })
                .collect();
            write!(self.out, "{head}\n{};\n", tuples.join(",\n")).map_err(|e| io(&e))?;
        }
        Ok(())
    }

    /// Closes the document and flushes; returns the writer's output.
    pub fn finish(mut self) -> Result<W, DbError> {
        if self.format == ExportFormat::Json {
            let end: &[u8] = if self.rows_written == 0 {
                b"]\n"
            } else {
                b"\n]\n"
            };
            self.out.write_all(end).map_err(|e| io(&e))?;
        }
        self.out.flush().map_err(|e| io(&e))?;
        Ok(self.out)
    }
}

#[cfg(test)]
mod tests;
