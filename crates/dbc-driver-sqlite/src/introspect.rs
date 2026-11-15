//! Introspection from `pragma_*` table functions, with the stored DDL text for the rest.

use dbc_core::dialect::quote_with;
use dbc_core::schema::{
    Column, ForeignKey, Index, Key, Namespace, NamespaceRef, ObjectKind, ObjectSummary,
    ReferentialAction, TableInfo, TableRef,
};
use dbc_core::{DbError, DbErrorCode};
use rusqlite::{Connection, OptionalExtension, params};

use crate::ddl::{parse_index, parse_table};
use crate::decode::category_for_declared;
use crate::errors::map_error;

const MAIN: &str = "main";

fn schema_of(namespace: &NamespaceRef) -> &str {
    namespace.schema.as_deref().unwrap_or(MAIN)
}

fn namespace(schema: &str) -> NamespaceRef {
    NamespaceRef {
        catalog: None,
        schema: Some(schema.to_owned()),
    }
}

/// `main`, `temp` (marked system) and every attached database.
pub(crate) fn list_namespaces(connection: &Connection) -> Result<Vec<Namespace>, DbError> {
    let mut statement = connection
        .prepare("SELECT name FROM pragma_database_list ORDER BY seq")
        .map_err(map_error)?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)?;
    Ok(names
        .into_iter()
        .map(|name| Namespace {
            namespace: namespace(&name),
            system: name == "temp",
            name,
        })
        .collect())
}

pub(crate) fn list_objects(
    connection: &Connection,
    namespace: &NamespaceRef,
) -> Result<Vec<ObjectSummary>, DbError> {
    let mut statement = connection
        .prepare(
            "SELECT name, type FROM pragma_table_list WHERE schema = ?1 \
             AND type IN ('table', 'view', 'virtual') AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' \
             ORDER BY name",
        )
        .map_err(map_error)?;
    let rows = statement
        .query_map([schema_of(namespace)], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(map_error)?;
    rows.map(|row| {
        let (name, kind) = row.map_err(map_error)?;
        Ok(ObjectSummary {
            name,
            kind: object_kind(&kind),
            comment: None,
        })
    })
    .collect()
}

fn object_kind(kind: &str) -> ObjectKind {
    if kind == "view" {
        ObjectKind::View
    } else {
        ObjectKind::Table
    }
}

/// `(type, without_rowid)` from `pragma_table_list`, or `NotFound`.
fn table_kind(connection: &Connection, table: &TableRef) -> Result<(ObjectKind, bool), DbError> {
    connection
        .query_row(
            "SELECT type, wr FROM pragma_table_list WHERE schema = ?1 AND name = ?2",
            params![schema_of(&table.namespace), table.name],
            |row| {
                Ok((
                    object_kind(&row.get::<_, String>(0)?),
                    row.get::<_, bool>(1)?,
                ))
            },
        )
        .optional()
        .map_err(map_error)?
        .ok_or_else(|| not_found(table))
}

fn not_found(table: &TableRef) -> DbError {
    DbError::new(
        DbErrorCode::NotFound,
        format!("{} does not exist", table.name),
    )
}

/// The `sql` column of `sqlite_master` for one object.
fn stored_sql(
    connection: &Connection,
    schema: &str,
    kind: &str,
    name: &str,
) -> Result<Option<String>, DbError> {
    let master = format!(
        "SELECT sql FROM {}.sqlite_master WHERE type = ?1 AND name = ?2",
        quote_with(schema, '"')
    );
    connection
        .query_row(&master, params![kind, name], |row| {
            row.get::<_, Option<String>>(0)
        })
        .optional()
        .map(Option::flatten)
        .map_err(map_error)
}

pub(crate) fn describe_table(
    connection: &Connection,
    table: &TableRef,
) -> Result<TableInfo, DbError> {
    let (kind, without_rowid) = table_kind(connection, table)?;
    let schema = schema_of(&table.namespace);
    let type_name = if kind == ObjectKind::View {
        "view"
    } else {
        "table"
    };
    let ddl = stored_sql(connection, schema, type_name, &table.name)?
        .map(|sql| parse_table(&sql))
        .unwrap_or_default();
    let (columns, primary_key) = columns(connection, table, without_rowid, &ddl.generated)?;
    let (indexes, unique_constraints) = indexes(connection, table)?;
    let mut info = TableInfo {
        table: table.clone(),
        kind,
        comment: None,
        columns,
        primary_key,
        indexes,
        foreign_keys: foreign_keys(connection, table)?,
        unique_constraints,
        check_constraints: ddl.checks,
        row_key: None,
        row_id: None,
    };
    if kind == ObjectKind::Table && !without_rowid {
        info.row_id = ["rowid", "_rowid_", "oid"]
            .into_iter()
            .find(|id| !info.columns.iter().any(|c| c.name.eq_ignore_ascii_case(id)))
            .map(str::to_owned);
    }
    info.row_key = info.choose_row_key();
    Ok(info)
}

struct RawColumn {
    name: String,
    declared: String,
    not_null: bool,
    default: Option<String>,
    pk: u32,
    hidden: u32,
}

fn columns(
    connection: &Connection,
    table: &TableRef,
    without_rowid: bool,
    generated: &[(String, String)],
) -> Result<(Vec<Column>, Option<Key>), DbError> {
    let mut statement = connection
        .prepare(
            "SELECT name, type, \"notnull\", dflt_value, pk, hidden \
             FROM pragma_table_xinfo(?1, ?2) WHERE hidden <> 1 ORDER BY cid",
        )
        .map_err(map_error)?;
    let raw = statement
        .query_map(params![table.name, schema_of(&table.namespace)], |row| {
            Ok(RawColumn {
                name: row.get(0)?,
                declared: row.get(1)?,
                not_null: row.get(2)?,
                default: row.get(3)?,
                pk: row.get(4)?,
                hidden: row.get(5)?,
            })
        })
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)?;
    let mut key: Vec<&RawColumn> = raw.iter().filter(|c| c.pk > 0).collect();
    key.sort_by_key(|c| c.pk);
    // A rowid alias (`INTEGER PRIMARY KEY`) and WITHOUT ROWID keys cannot hold NULL.
    let rowid_alias = key.len() == 1 && key[0].declared.eq_ignore_ascii_case("INTEGER");
    let key_not_null = without_rowid || rowid_alias;
    let primary_key = (!key.is_empty()).then(|| Key {
        name: None,
        columns: key.iter().map(|c| c.name.clone()).collect(),
    });
    let columns = raw
        .iter()
        .zip(1u32..)
        .map(|(c, ordinal)| {
            let is_generated = c.hidden >= 2;
            let expression = generated
                .iter()
                .find(|(name, _)| name == &c.name)
                .map(|(_, e)| e.clone());
            Column {
                name: c.name.clone(),
                type_name: c.declared.clone(),
                category: category_for_declared(&c.declared),
                nullable: !(c.not_null || (c.pk > 0 && key_not_null)),
                default: if is_generated {
                    expression
                } else {
                    c.default.clone()
                },
                comment: None,
                ordinal,
                generated: is_generated,
            }
        })
        .collect();
    Ok((columns, primary_key))
}

/// Indexes from `CREATE INDEX` (origin `c`) and unique constraints (origin `u`); the primary
/// key's own index is not listed.
fn indexes(connection: &Connection, table: &TableRef) -> Result<(Vec<Index>, Vec<Key>), DbError> {
    let schema = schema_of(&table.namespace);
    let mut statement = connection
        .prepare(
            "SELECT name, \"unique\", origin, partial FROM pragma_index_list(?1, ?2) ORDER BY name",
        )
        .map_err(map_error)?;
    let listed = statement
        .query_map(params![table.name, schema], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)?;
    let mut indexes = Vec::new();
    let mut unique = Vec::new();
    for (name, is_unique, origin, partial) in listed {
        let key_columns = index_columns(connection, schema, &name)?;
        match origin.as_str() {
            "u" => unique.push(Key {
                name: None,
                columns: key_columns
                    .into_iter()
                    .map(|(_, column)| column.unwrap_or_default())
                    .collect(),
            }),
            "c" => {
                let ddl = stored_sql(connection, schema, "index", &name)?
                    .map(|sql| parse_index(&sql))
                    .unwrap_or_default();
                let columns = key_columns
                    .into_iter()
                    .map(|(seq, column)| {
                        column.unwrap_or_else(|| ddl.parts.get(seq).cloned().unwrap_or_default())
                    })
                    .collect();
                indexes.push(Index {
                    name,
                    columns,
                    unique: is_unique,
                    method: None,
                    predicate: if partial { ddl.predicate } else { None },
                });
            }
            _ => {}
        }
    }
    Ok((indexes, unique))
}

/// `(position, column name)`; the name is `None` for an expression.
fn index_columns(
    connection: &Connection,
    schema: &str,
    index: &str,
) -> Result<Vec<(usize, Option<String>)>, DbError> {
    let mut statement = connection
        .prepare("SELECT seqno, name FROM pragma_index_info(?1, ?2) ORDER BY seqno")
        .map_err(map_error)?;
    statement
        .query_map(params![index, schema], |row| {
            let seq: i64 = row.get(0)?;
            Ok((
                usize::try_from(seq).unwrap_or(usize::MAX),
                row.get::<_, Option<String>>(1)?,
            ))
        })
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)
}

fn foreign_keys(connection: &Connection, table: &TableRef) -> Result<Vec<ForeignKey>, DbError> {
    let mut statement = connection
        .prepare(
            "SELECT id, \"table\", \"from\", \"to\", on_update, on_delete \
             FROM pragma_foreign_key_list(?1, ?2) ORDER BY id DESC, seq",
        )
        .map_err(map_error)?;
    let rows = statement
        .query_map(params![table.name, schema_of(&table.namespace)], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)?;
    let mut keys: Vec<(i64, ForeignKey)> = Vec::new();
    for (id, target, from, to, on_update, on_delete) in rows {
        if keys.last().is_none_or(|(last, _)| *last != id) {
            keys.push((
                id,
                ForeignKey {
                    name: None,
                    columns: Vec::new(),
                    referenced_table: TableRef {
                        namespace: table.namespace.clone(),
                        name: target,
                    },
                    referenced_columns: Vec::new(),
                    on_update: ReferentialAction::parse(&on_update),
                    on_delete: ReferentialAction::parse(&on_delete),
                },
            ));
        }
        if let Some((_, key)) = keys.last_mut() {
            key.columns.push(from);
            key.referenced_columns.extend(to);
        }
    }
    keys.into_iter()
        .map(|(_, key)| referencing_primary_key(connection, key))
        .collect()
}

/// `REFERENCES t` without columns means the referenced table's primary key.
fn referencing_primary_key(
    connection: &Connection,
    key: ForeignKey,
) -> Result<ForeignKey, DbError> {
    if !key.referenced_columns.is_empty() {
        return Ok(key);
    }
    let mut statement = connection
        .prepare("SELECT name FROM pragma_table_info(?1, ?2) WHERE pk > 0 ORDER BY pk")
        .map_err(map_error)?;
    let columns = statement
        .query_map(
            params![
                key.referenced_table.name,
                schema_of(&key.referenced_table.namespace)
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)?;
    Ok(ForeignKey {
        referenced_columns: columns,
        ..key
    })
}

/// The stored `CREATE` text of the table or view and of its indexes, as SQLite keeps it.
pub(crate) fn table_ddl(connection: &Connection, table: &TableRef) -> Result<String, DbError> {
    let (kind, _) = table_kind(connection, table)?;
    let schema = schema_of(&table.namespace);
    let type_name = if kind == ObjectKind::View {
        "view"
    } else {
        "table"
    };
    let mut statements: Vec<String> = stored_sql(connection, schema, type_name, &table.name)?
        .into_iter()
        .collect();
    let query = format!(
        "SELECT sql FROM {}.sqlite_master WHERE type IN ('index', 'trigger') AND tbl_name = ?1 \
         AND sql IS NOT NULL ORDER BY type, name",
        quote_with(schema, '"')
    );
    let mut statement = connection.prepare(&query).map_err(map_error)?;
    let rows = statement
        .query_map([&table.name], |row| row.get::<_, String>(0))
        .map_err(map_error)?;
    for sql in rows {
        statements.push(sql.map_err(map_error)?);
    }
    Ok(statements
        .into_iter()
        .map(|sql| format!("{sql};\n"))
        .collect::<Vec<_>>()
        .join("\n"))
}
