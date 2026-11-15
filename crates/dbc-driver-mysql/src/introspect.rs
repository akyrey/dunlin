//! Introspection from `information_schema`, and DDL from `SHOW CREATE TABLE`.
//! MySQL and MariaDB differ in `CHECK_CONSTRAINTS`, `STATISTICS.EXPRESSION` and how
//! `COLUMN_DEFAULT` is spelled; [`Flavour`] picks the right query.

use dbc_core::dialect::quote_with;
use dbc_core::result::TypeCategory;
use dbc_core::schema::{
    Check, Column, ForeignKey, Index, Key, Namespace, NamespaceRef, ObjectKind, ObjectSummary,
    ReferentialAction, TableInfo, TableRef,
};
use dbc_core::{DbError, DbErrorCode};
use mysql_async::prelude::Queryable;
use mysql_async::{Conn, Row};

use crate::errors::map_error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flavour {
    Mysql,
    Mariadb,
}

impl Flavour {
    pub(crate) async fn detect(conn: &mut Conn) -> Result<Self, DbError> {
        let version: Option<String> = conn
            .query_first("SELECT VERSION()")
            .await
            .map_err(|e| map_error(&e))?;
        Ok(if version.unwrap_or_default().contains("MariaDB") {
            Self::Mariadb
        } else {
            Self::Mysql
        })
    }
}

const SYSTEM_SCHEMAS: [&str; 4] = ["information_schema", "mysql", "performance_schema", "sys"];

fn namespace(schema: &str) -> NamespaceRef {
    NamespaceRef {
        catalog: None,
        schema: Some(schema.to_owned()),
    }
}

fn schema_of(namespace: &NamespaceRef) -> &str {
    namespace.schema.as_deref().unwrap_or_default()
}

/// Rows of `sql` with string parameters, through a prepared statement.
async fn rows(conn: &mut Conn, sql: &str, params: Vec<String>) -> Result<Vec<Row>, DbError> {
    conn.exec(sql, params).await.map_err(|e| map_error(&e))
}

fn text(row: &Row, index: usize) -> Option<String> {
    match row.as_ref(index) {
        Some(mysql_async::Value::Bytes(bytes)) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Some(mysql_async::Value::Int(i)) => Some(i.to_string()),
        Some(mysql_async::Value::UInt(i)) => Some(i.to_string()),
        _ => None,
    }
}

fn text_or_empty(row: &Row, index: usize) -> String {
    text(row, index).unwrap_or_default()
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.is_empty())
}

pub(crate) async fn list_namespaces(conn: &mut Conn) -> Result<Vec<Namespace>, DbError> {
    let found = rows(
        conn,
        "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME",
        Vec::new(),
    )
    .await?;
    Ok(found
        .iter()
        .map(|row| {
            let name = text_or_empty(row, 0);
            Namespace {
                namespace: namespace(&name),
                system: SYSTEM_SCHEMAS.contains(&name.to_ascii_lowercase().as_str()),
                name,
            }
        })
        .collect())
}

fn object_kind(table_type: &str) -> Option<ObjectKind> {
    match table_type {
        "VIEW" | "SYSTEM VIEW" => Some(ObjectKind::View),
        "BASE TABLE" | "SYSTEM VERSIONED" => Some(ObjectKind::Table),
        _ => None,
    }
}

/// MySQL puts "VIEW" in the comment of every view.
fn table_comment(kind: ObjectKind, comment: Option<String>) -> Option<String> {
    non_empty(comment).filter(|c| !(kind == ObjectKind::View && c == "VIEW"))
}

pub(crate) async fn list_objects(
    conn: &mut Conn,
    namespace: &NamespaceRef,
) -> Result<Vec<ObjectSummary>, DbError> {
    let schema = schema_of(namespace).to_owned();
    let exists = rows(
        conn,
        "SELECT 1 FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = ?",
        vec![schema.clone()],
    )
    .await?;
    if exists.is_empty() {
        return Err(DbError::new(
            DbErrorCode::NotFound,
            format!("Database {schema} does not exist"),
        ));
    }
    let found = rows(
        conn,
        "SELECT TABLE_NAME, TABLE_TYPE, TABLE_COMMENT FROM information_schema.TABLES \
         WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
        vec![schema],
    )
    .await?;
    Ok(found
        .iter()
        .filter_map(|row| {
            let kind = object_kind(&text_or_empty(row, 1))?;
            Some(ObjectSummary {
                name: text_or_empty(row, 0),
                kind,
                comment: table_comment(kind, text(row, 2)),
            })
        })
        .collect())
}

async fn table_kind(
    conn: &mut Conn,
    table: &TableRef,
) -> Result<(ObjectKind, Option<String>), DbError> {
    let found = rows(
        conn,
        "SELECT TABLE_TYPE, TABLE_COMMENT FROM information_schema.TABLES \
         WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?",
        vec![schema_of(&table.namespace).to_owned(), table.name.clone()],
    )
    .await?;
    let kind = found
        .first()
        .and_then(|row| object_kind(&text_or_empty(row, 0)));
    match (kind, found.first()) {
        (Some(kind), Some(row)) => Ok((kind, table_comment(kind, text(row, 1)))),
        _ => Err(DbError::new(
            DbErrorCode::NotFound,
            format!("{} does not exist", table.name),
        )),
    }
}

fn params(table: &TableRef) -> Vec<String> {
    vec![schema_of(&table.namespace).to_owned(), table.name.clone()]
}

pub(crate) async fn describe_table(
    conn: &mut Conn,
    flavour: Flavour,
    table: &TableRef,
) -> Result<TableInfo, DbError> {
    let (kind, comment) = table_kind(conn, table).await?;
    let columns = columns(conn, table).await?;
    let (primary_key, unique_constraints, foreign_keys) = keys(conn, table).await?;
    let indexes = indexes(conn, flavour, table, &unique_constraints).await?;
    let mut info = TableInfo {
        table: table.clone(),
        kind,
        comment,
        columns,
        primary_key,
        indexes,
        foreign_keys,
        unique_constraints,
        check_constraints: checks(conn, flavour, table).await?,
        row_key: None,
        row_id: None,
    };
    info.row_key = info.choose_row_key();
    Ok(info)
}

async fn columns(conn: &mut Conn, table: &TableRef) -> Result<Vec<Column>, DbError> {
    let found = rows(
        conn,
        "SELECT COLUMN_NAME, COLUMN_TYPE, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT, COLUMN_COMMENT, \
                EXTRA, GENERATION_EXPRESSION \
         FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
         ORDER BY ORDINAL_POSITION",
        params(table),
    )
    .await?;
    Ok(found
        .iter()
        .zip(1u32..)
        .map(|(row, ordinal)| {
            let column_type = text_or_empty(row, 1);
            // MySQL also reports `DEFAULT_GENERATED` for `DEFAULT CURRENT_TIMESTAMP`.
            let extra = text_or_empty(row, 6).to_ascii_uppercase();
            let generated =
                extra.contains("STORED GENERATED") || extra.contains("VIRTUAL GENERATED");
            let default = if generated {
                // MySQL escapes quotes in GENERATION_EXPRESSION.
                non_empty(text(row, 7)).map(|e| e.replace("\\'", "'"))
            } else {
                // MariaDB spells "no default" as the string NULL.
                text(row, 4).filter(|d| d != "NULL")
            };
            Column {
                name: text_or_empty(row, 0),
                category: category(&text_or_empty(row, 2), &column_type),
                type_name: column_type,
                nullable: text_or_empty(row, 3) == "YES",
                default,
                comment: non_empty(text(row, 5)),
                ordinal,
                generated,
            }
        })
        .collect())
}

/// Category from `DATA_TYPE`; `tinyint(1)` is how MySQL stores `BOOLEAN`.
pub(crate) fn category(data_type: &str, column_type: &str) -> TypeCategory {
    let column_type = column_type.to_ascii_lowercase();
    match data_type.to_ascii_lowercase().as_str() {
        "tinyint" if column_type.starts_with("tinyint(1)") => TypeCategory::Bool,
        "tinyint" | "smallint" | "mediumint" | "int" | "integer" | "bigint" | "year" => {
            TypeCategory::Integer
        }
        "decimal" | "numeric" => TypeCategory::Decimal,
        "float" | "double" | "real" => TypeCategory::Float,
        "char" | "varchar" | "tinytext" | "text" | "mediumtext" | "longtext" | "enum" | "set" => {
            TypeCategory::Text
        }
        "json" => TypeCategory::Json,
        "binary" | "varbinary" | "tinyblob" | "blob" | "mediumblob" | "longblob" => {
            TypeCategory::Binary
        }
        "date" | "time" | "datetime" | "timestamp" => TypeCategory::Temporal,
        "uuid" => TypeCategory::Uuid,
        _ => TypeCategory::Other,
    }
}

type Keys = (Option<Key>, Vec<Key>, Vec<ForeignKey>);

async fn keys(conn: &mut Conn, table: &TableRef) -> Result<Keys, DbError> {
    let found = rows(
        conn,
        "SELECT tc.CONSTRAINT_NAME, tc.CONSTRAINT_TYPE, kcu.COLUMN_NAME, \
                kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, \
                rc.UPDATE_RULE, rc.DELETE_RULE \
         FROM information_schema.TABLE_CONSTRAINTS tc \
         JOIN information_schema.KEY_COLUMN_USAGE kcu \
           ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME \
          AND kcu.TABLE_NAME = tc.TABLE_NAME \
         LEFT JOIN information_schema.REFERENTIAL_CONSTRAINTS rc \
           ON rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME \
          AND rc.TABLE_NAME = tc.TABLE_NAME \
         WHERE tc.TABLE_SCHEMA = ? AND tc.TABLE_NAME = ? \
           AND tc.CONSTRAINT_TYPE IN ('PRIMARY KEY', 'UNIQUE', 'FOREIGN KEY') \
         ORDER BY tc.CONSTRAINT_TYPE, tc.CONSTRAINT_NAME, kcu.ORDINAL_POSITION",
        params(table),
    )
    .await?;
    let mut primary: Option<Key> = None;
    let mut unique: Vec<Key> = Vec::new();
    let mut foreign: Vec<ForeignKey> = Vec::new();
    for row in &found {
        let name = text_or_empty(row, 0);
        let column = text_or_empty(row, 2);
        match text_or_empty(row, 1).as_str() {
            "PRIMARY KEY" => primary
                .get_or_insert_with(|| empty_key(&name))
                .columns
                .push(column),
            "UNIQUE" => {
                if unique
                    .last()
                    .is_none_or(|k| k.name.as_deref() != Some(name.as_str()))
                {
                    unique.push(empty_key(&name));
                }
                if let Some(key) = unique.last_mut() {
                    key.columns.push(column);
                }
            }
            _ => {
                if foreign
                    .last()
                    .is_none_or(|k| k.name.as_deref() != Some(name.as_str()))
                {
                    foreign.push(ForeignKey {
                        name: Some(name.clone()),
                        columns: Vec::new(),
                        referenced_table: TableRef {
                            namespace: namespace(&text_or_empty(row, 3)),
                            name: text_or_empty(row, 4),
                        },
                        referenced_columns: Vec::new(),
                        on_update: action(text(row, 6)),
                        on_delete: action(text(row, 7)),
                    });
                }
                if let Some(key) = foreign.last_mut() {
                    key.columns.push(column);
                    key.referenced_columns.push(text_or_empty(row, 5));
                }
            }
        }
    }
    Ok((primary, unique, foreign))
}

fn empty_key(name: &str) -> Key {
    Key {
        name: Some(name.to_owned()),
        columns: Vec::new(),
    }
}

/// InnoDB treats `RESTRICT` and `NO ACTION` alike, and the two servers report the default
/// differently, so both are `None`.
fn action(rule: Option<String>) -> Option<ReferentialAction> {
    rule.as_deref()
        .and_then(ReferentialAction::parse)
        .filter(|a| *a != ReferentialAction::Restrict)
}

/// Indexes other than the primary key and unique constraints.
async fn indexes(
    conn: &mut Conn,
    flavour: Flavour,
    table: &TableRef,
    unique: &[Key],
) -> Result<Vec<Index>, DbError> {
    let expression = match flavour {
        Flavour::Mysql => "EXPRESSION",
        Flavour::Mariadb => "NULL",
    };
    let sql = format!(
        "SELECT INDEX_NAME, NON_UNIQUE, COLUMN_NAME, {expression}, INDEX_TYPE \
         FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
         ORDER BY INDEX_NAME, SEQ_IN_INDEX"
    );
    let found = rows(conn, &sql, params(table)).await?;
    let mut indexes: Vec<Index> = Vec::new();
    for row in &found {
        let name = text_or_empty(row, 0);
        let is_constraint = name == "PRIMARY"
            || unique
                .iter()
                .any(|k| k.name.as_deref() == Some(name.as_str()));
        if is_constraint {
            continue;
        }
        if indexes.last().is_none_or(|i| i.name != name) {
            indexes.push(Index {
                name: name.clone(),
                columns: Vec::new(),
                unique: text_or_empty(row, 1) == "0",
                method: non_empty(text(row, 4)).map(|m| m.to_ascii_lowercase()),
                predicate: None,
            });
        }
        let part = text(row, 2).or_else(|| text(row, 3)).unwrap_or_default();
        if let Some(index) = indexes.last_mut() {
            index.columns.push(part);
        }
    }
    Ok(indexes)
}

async fn checks(
    conn: &mut Conn,
    flavour: Flavour,
    table: &TableRef,
) -> Result<Vec<Check>, DbError> {
    // MariaDB names column checks after the column, so names repeat across tables; it has a
    // TABLE_NAME column for that. MySQL names are unique in the schema.
    let sql = match flavour {
        Flavour::Mariadb => {
            "SELECT CONSTRAINT_NAME, CHECK_CLAUSE FROM information_schema.CHECK_CONSTRAINTS \
             WHERE CONSTRAINT_SCHEMA = ? AND TABLE_NAME = ? ORDER BY CONSTRAINT_NAME"
        }
        Flavour::Mysql => {
            "SELECT cc.CONSTRAINT_NAME, cc.CHECK_CLAUSE FROM information_schema.CHECK_CONSTRAINTS cc \
             JOIN information_schema.TABLE_CONSTRAINTS tc \
               ON tc.CONSTRAINT_SCHEMA = cc.CONSTRAINT_SCHEMA AND tc.CONSTRAINT_NAME = cc.CONSTRAINT_NAME \
             WHERE tc.TABLE_SCHEMA = ? AND tc.TABLE_NAME = ? AND tc.CONSTRAINT_TYPE = 'CHECK' \
             ORDER BY cc.CONSTRAINT_NAME"
        }
    };
    let found = rows(conn, sql, params(table)).await?;
    Ok(found
        .iter()
        .map(|row| Check {
            name: non_empty(text(row, 0)),
            expression: text_or_empty(row, 1),
        })
        .collect())
}

/// The server's own `SHOW CREATE TABLE` (or `SHOW CREATE VIEW`) text.
pub(crate) async fn table_ddl(conn: &mut Conn, table: &TableRef) -> Result<String, DbError> {
    let (kind, _) = table_kind(conn, table).await?;
    let name = format!(
        "{}.{}",
        quote_with(schema_of(&table.namespace), '`'),
        quote_with(&table.name, '`')
    );
    let statement = match kind {
        ObjectKind::View => format!("SHOW CREATE VIEW {name}"),
        _ => format!("SHOW CREATE TABLE {name}"),
    };
    let row: Option<Row> = conn
        .query_first(statement)
        .await
        .map_err(|e| map_error(&e))?;
    let ddl = row.and_then(|row| text(&row, 1)).unwrap_or_default();
    Ok(format!("{ddl};\n"))
}
