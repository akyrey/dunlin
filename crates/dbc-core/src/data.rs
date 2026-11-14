//! Table browsing (Phase 6): one page of a table, row counts, one full cell value.
//!
//! The statements are built here from the dialect, the table's description and the request, so
//! every driver shares them (D-022). Identifiers are quoted by the dialect; key values are bound
//! parameters. The filter is the user's own `WHERE` expression, like SQL typed in the editor:
//! it is checked to be a single statement and wrapped in parentheses, never edited.

use serde::{Deserialize, Serialize};

use crate::dialect::Dialect;
use crate::driver::Connection;
use crate::error::{DbError, DbErrorCode};
use crate::result::{ExecuteOptions, ResultSet, StatementOutcome};
use crate::schema::{TableInfo, TableRef};
use crate::sql;
use crate::value::Value;

/// Rows per page unless the request says otherwise.
pub const DEFAULT_PAGE_SIZE: usize = 200;
/// Upper bound for one page.
pub const MAX_PAGE_SIZE: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SortKey {
    pub column: String,
    pub descending: bool,
}

/// `column = value`, bound as a parameter (foreign key navigation, cell lookups).
#[derive(Debug, Clone, PartialEq)]
pub struct KeyValue {
    pub column: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TablePageRequest {
    pub table: TableRef,
    /// A raw `WHERE` expression typed by the user.
    pub filter: Option<String>,
    /// Equality conditions on columns, combined with the filter.
    pub key: Vec<KeyValue>,
    /// Empty: row key order, when the table has one.
    pub sort: Vec<SortKey>,
    pub limit: usize,
    pub offset: u64,
}

/// Counts cross IPC as strings, like every 64-bit number (D-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", content = "rows", rename_all = "snake_case")]
pub enum RowCount {
    Exact(
        #[serde(with = "as_string")]
        #[cfg_attr(feature = "ts", ts(type = "string"))]
        u64,
    ),
    Estimate(
        #[serde(with = "as_string")]
        #[cfg_attr(feature = "ts", ts(type = "string"))]
        u64,
    ),
    Unknown,
}

mod as_string {
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(value)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// A statement and the values for its placeholders, in order.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundSql {
    pub sql: String,
    pub params: Vec<Value>,
}

fn invalid(code: DbErrorCode, message: impl Into<String>) -> DbError {
    DbError::new(code, message)
}

fn column_type<'a>(info: &'a TableInfo, column: &str) -> Result<&'a str, DbError> {
    info.columns
        .iter()
        .find(|c| c.name == column)
        .map(|c| c.type_name.as_str())
        .ok_or_else(|| {
            invalid(
                DbErrorCode::NotFound,
                format!("{} has no column {column}", info.table.name),
            )
        })
}

/// `WHERE (<filter>) AND "a" = $1 ...`, or nothing.
fn where_clause<D: Dialect + ?Sized>(
    dialect: &D,
    info: &TableInfo,
    filter: Option<&str>,
    key: &[KeyValue],
    params: &mut Vec<Value>,
) -> Result<String, DbError> {
    let mut conditions = Vec::new();
    if let Some(filter) = filter.map(str::trim).filter(|f| !f.is_empty()) {
        let statements = sql::split(filter, dialect.splitter_rules());
        if statements.len() > 1 || filter.trim_end().ends_with(';') {
            return Err(invalid(
                DbErrorCode::Syntax,
                "The filter must be one expression, without ';'",
            ));
        }
        conditions.push(format!("({filter})"));
    }
    for KeyValue { column, value } in key {
        let type_name = column_type(info, column)?;
        let quoted = dialect.quote_identifier(column);
        if *value == Value::Null {
            conditions.push(format!("{quoted} IS NULL"));
            continue;
        }
        params.push(value.clone());
        conditions.push(format!(
            "{quoted} = {}",
            dialect.placeholder(params.len(), type_name)
        ));
    }
    Ok(if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    })
}

/// `SELECT * FROM <table> [WHERE ...] [ORDER BY ...] LIMIT n OFFSET m`. Sort columns must be
/// columns of the table; without a sort, rows come in row key order when there is one.
pub fn page_sql<D: Dialect + ?Sized>(
    dialect: &D,
    info: &TableInfo,
    request: &TablePageRequest,
) -> Result<BoundSql, DbError> {
    let mut bound = select_sql(dialect, info, request, None)?;
    let limit = request.limit.clamp(1, MAX_PAGE_SIZE);
    bound
        .sql
        .push_str(&format!(" LIMIT {limit} OFFSET {}", request.offset));
    Ok(bound)
}

/// The rows `request` selects (filter, key, sort), without `LIMIT`: a whole-table export
/// (task 8.3). `columns` lists the columns to select, `*` when `None`.
pub fn select_sql<D: Dialect + ?Sized>(
    dialect: &D,
    info: &TableInfo,
    request: &TablePageRequest,
    columns: Option<&[String]>,
) -> Result<BoundSql, DbError> {
    let mut params = Vec::new();
    let where_clause = where_clause(
        dialect,
        info,
        request.filter.as_deref(),
        &request.key,
        &mut params,
    )?;
    let mut order = Vec::new();
    for key in &request.sort {
        column_type(info, &key.column)?;
        let direction = if key.descending { " DESC" } else { "" };
        order.push(format!(
            "{}{direction}",
            dialect.quote_identifier(&key.column)
        ));
    }
    if order.is_empty() {
        let row_key = info.row_key.as_deref().unwrap_or_default();
        order.extend(row_key.iter().map(|c| dialect.quote_identifier(c)));
    }
    let order_by = if order.is_empty() {
        String::new()
    } else {
        format!(" ORDER BY {}", order.join(", "))
    };
    let list = match columns {
        Some(names) => {
            let quoted = names
                .iter()
                .map(|name| column_type(info, name).map(|_| dialect.quote_identifier(name)))
                .collect::<Result<Vec<_>, _>>()?;
            quoted.join(", ")
        }
        None => "*".to_owned(),
    };
    let sql = format!(
        "SELECT {list} FROM {}{where_clause}{order_by}",
        dialect.qualified_name(&request.table)
    );
    Ok(BoundSql { sql, params })
}

/// `SELECT count(*) FROM <table> [WHERE ...]`.
pub fn count_sql<D: Dialect + ?Sized>(
    dialect: &D,
    info: &TableInfo,
    filter: Option<&str>,
    key: &[KeyValue],
) -> Result<BoundSql, DbError> {
    let mut params = Vec::new();
    let where_clause = where_clause(dialect, info, filter, key, &mut params)?;
    let sql = format!(
        "SELECT count(*) FROM {}{where_clause}",
        dialect.qualified_name(&info.table)
    );
    Ok(BoundSql { sql, params })
}

/// `SELECT <column> FROM <table> WHERE <row key> = ...`, for a value truncated on the wire.
pub fn cell_sql<D: Dialect + ?Sized>(
    dialect: &D,
    info: &TableInfo,
    key: &[KeyValue],
    column: &str,
) -> Result<BoundSql, DbError> {
    column_type(info, column)?;
    let row_key = info.row_key.as_deref().unwrap_or_default();
    let mut given: Vec<&str> = key.iter().map(|k| k.column.as_str()).collect();
    let mut wanted: Vec<&str> = row_key.iter().map(String::as_str).collect();
    given.sort_unstable();
    wanted.sort_unstable();
    if wanted.is_empty() || given != wanted {
        return Err(invalid(
            DbErrorCode::Unsupported,
            "The row cannot be addressed: the table has no primary key or unique not-null key",
        ));
    }
    let mut params = Vec::new();
    let where_clause = where_clause(dialect, info, None, key, &mut params)?;
    let sql = format!(
        "SELECT {} FROM {}{where_clause}",
        dialect.quote_identifier(column),
        dialect.qualified_name(&info.table)
    );
    Ok(BoundSql { sql, params })
}

fn rows(outcome: StatementOutcome) -> Result<ResultSet, DbError> {
    match outcome {
        StatementOutcome::Rows(result) => Ok(result),
        StatementOutcome::Affected { .. } => {
            Err(invalid(DbErrorCode::Internal, "The query returned no rows"))
        }
    }
}

/// One page of the table.
pub async fn fetch_page(
    connection: &dyn Connection,
    dialect: &dyn Dialect,
    info: &TableInfo,
    request: &TablePageRequest,
) -> Result<ResultSet, DbError> {
    let bound = page_sql(dialect, info, request)?;
    let options = ExecuteOptions {
        row_limit: request.limit.clamp(1, MAX_PAGE_SIZE),
    };
    rows(
        connection
            .query_params(&bound.sql, &bound.params, &options)
            .await?,
    )
}

/// An exact count when asked for, else the driver's estimate (only without a filter).
pub async fn count_rows(
    connection: &dyn Connection,
    dialect: &dyn Dialect,
    info: &TableInfo,
    filter: Option<&str>,
    key: &[KeyValue],
    exact: bool,
) -> Result<RowCount, DbError> {
    let unfiltered = filter.is_none_or(|f| f.trim().is_empty()) && key.is_empty();
    if !exact {
        if !unfiltered {
            return Ok(RowCount::Unknown);
        }
        return Ok(connection
            .estimate_rows(&info.table)
            .await?
            .map_or(RowCount::Unknown, RowCount::Estimate));
    }
    let bound = count_sql(dialect, info, filter, key)?;
    let result = rows(
        connection
            .query_params(&bound.sql, &bound.params, &ExecuteOptions { row_limit: 1 })
            .await?,
    )?;
    let count = match result.rows.first().and_then(|row| row.first()) {
        Some(Value::Int(n)) => u64::try_from(*n).ok(),
        Some(Value::UInt(n)) => Some(*n),
        Some(Value::Decimal(text) | Value::Text(text)) => text.parse().ok(),
        _ => None,
    };
    count
        .map(RowCount::Exact)
        .ok_or_else(|| invalid(DbErrorCode::Internal, "count(*) returned no number"))
}

/// The full value of one cell, addressed by the row key.
pub async fn fetch_cell(
    connection: &dyn Connection,
    dialect: &dyn Dialect,
    info: &TableInfo,
    key: &[KeyValue],
    column: &str,
) -> Result<Value, DbError> {
    let bound = cell_sql(dialect, info, key, column)?;
    let result = rows(
        connection
            .query_params(&bound.sql, &bound.params, &ExecuteOptions { row_limit: 1 })
            .await?,
    )?;
    result
        .rows
        .into_iter()
        .next()
        .and_then(|row| row.into_iter().next())
        .ok_or_else(|| invalid(DbErrorCode::NotFound, "The row no longer exists"))
}

#[cfg(test)]
mod tests;
