//! Introspection from the system catalogs (`pg_class`, `pg_attribute`, `pg_constraint`,
//! `pg_index`), which are faster and more complete than `information_schema`.

use dbc_core::result::TypeCategory;
use dbc_core::schema::{
    Check, Column, ForeignKey, Index, Key, Namespace, NamespaceRef, ObjectKind, ObjectSummary,
    ReferentialAction, TableInfo, TableRef,
};
use dbc_core::{DbError, DbErrorCode};
use tokio_postgres::types::{Oid, Type};
use tokio_postgres::{Client, Row};

use crate::errors::map_error;

/// Relation kinds shown in the tree: tables, partitioned tables, foreign tables, views and
/// materialized views.
const RELKINDS: &str = "('r', 'p', 'f', 'v', 'm')";

const NAMESPACES: &str = "\
    SELECT nspname::text, nspname LIKE 'pg\\_%' OR nspname = 'information_schema' \
    FROM pg_namespace WHERE nspname !~ '^pg_(toast_)?temp_' ORDER BY nspname";

pub(crate) async fn list_namespaces(client: &Client) -> Result<Vec<Namespace>, DbError> {
    let rows = query(client, NAMESPACES, &[]).await?;
    rows.iter()
        .map(|row| {
            let name: String = get(row, 0)?;
            Ok(Namespace {
                namespace: namespace(&name),
                system: get(row, 1)?,
                name,
            })
        })
        .collect()
}

fn namespace(schema: &str) -> NamespaceRef {
    NamespaceRef {
        catalog: None,
        schema: Some(schema.to_owned()),
    }
}

fn schema_of(namespace: &NamespaceRef) -> &str {
    namespace.schema.as_deref().unwrap_or("public")
}

pub(crate) async fn list_objects(
    client: &Client,
    namespace: &NamespaceRef,
) -> Result<Vec<ObjectSummary>, DbError> {
    let schema = schema_of(namespace);
    let access = query(
        client,
        "SELECT has_schema_privilege(oid, 'USAGE') FROM pg_namespace WHERE nspname = $1",
        &[&schema],
    )
    .await?;
    let Some(row) = access.first() else {
        return Err(DbError::new(
            DbErrorCode::NotFound,
            format!("Schema {schema} does not exist"),
        ));
    };
    if !get::<bool>(row, 0)? {
        return Err(DbError::new(
            DbErrorCode::Permission,
            format!("permission denied for schema {schema}"),
        ));
    }
    let sql = format!(
        "SELECT c.relname::text, c.relkind::text, obj_description(c.oid, 'pg_class') \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relkind IN {RELKINDS} AND NOT c.relispartition \
         ORDER BY c.relname"
    );
    let rows = query(client, &sql, &[&schema]).await?;
    rows.iter()
        .map(|row| {
            Ok(ObjectSummary {
                name: get(row, 0)?,
                kind: object_kind(&get::<String>(row, 1)?),
                comment: get(row, 2)?,
            })
        })
        .collect()
}

fn object_kind(relkind: &str) -> ObjectKind {
    match relkind {
        "v" => ObjectKind::View,
        "m" => ObjectKind::MaterializedView,
        _ => ObjectKind::Table,
    }
}

/// The relation's oid, kind and comment.
pub(crate) struct Relation {
    pub oid: Oid,
    pub relkind: String,
    pub comment: Option<String>,
}

pub(crate) async fn relation(client: &Client, table: &TableRef) -> Result<Relation, DbError> {
    let sql = format!(
        "SELECT c.oid, c.relkind::text, obj_description(c.oid, 'pg_class') \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN {RELKINDS}"
    );
    let rows = query(client, &sql, &[&schema_of(&table.namespace), &table.name]).await?;
    let row = rows.first().ok_or_else(|| {
        DbError::new(
            DbErrorCode::NotFound,
            format!("{} does not exist", table.name),
        )
    })?;
    Ok(Relation {
        oid: get(row, 0)?,
        relkind: get(row, 1)?,
        comment: get(row, 2)?,
    })
}

pub(crate) async fn describe_table(
    client: &Client,
    table: &TableRef,
) -> Result<TableInfo, DbError> {
    let relation = relation(client, table).await?;
    let columns = columns(client, relation.oid).await?;
    let constraints = constraints(client, relation.oid).await?;
    let mut info = TableInfo {
        table: table.clone(),
        kind: object_kind(&relation.relkind),
        comment: relation.comment,
        columns: columns.into_iter().map(|c| c.column).collect(),
        primary_key: None,
        indexes: indexes(client, relation.oid)
            .await?
            .into_iter()
            .map(|i| i.index)
            .collect(),
        foreign_keys: Vec::new(),
        unique_constraints: Vec::new(),
        check_constraints: Vec::new(),
        row_key: None,
    };
    for constraint in constraints {
        let name = Some(constraint.name);
        match constraint.kind.as_str() {
            "p" => {
                info.primary_key = Some(Key {
                    name,
                    columns: constraint.columns,
                })
            }
            "u" => info.unique_constraints.push(Key {
                name,
                columns: constraint.columns,
            }),
            "c" => info.check_constraints.push(Check {
                name,
                expression: constraint.check.unwrap_or_default(),
            }),
            "f" => info.foreign_keys.push(ForeignKey {
                name,
                columns: constraint.columns,
                referenced_table: TableRef {
                    namespace: namespace(&constraint.referenced_schema.unwrap_or_default()),
                    name: constraint.referenced_table.unwrap_or_default(),
                },
                referenced_columns: constraint.referenced_columns,
                on_update: action(&constraint.on_update),
                on_delete: action(&constraint.on_delete),
            }),
            _ => {}
        }
    }
    info.row_key = info.choose_row_key();
    Ok(info)
}

fn action(code: &str) -> Option<ReferentialAction> {
    match code {
        "c" => Some(ReferentialAction::Cascade),
        "n" => Some(ReferentialAction::SetNull),
        "d" => Some(ReferentialAction::SetDefault),
        "r" => Some(ReferentialAction::Restrict),
        _ => None,
    }
}

/// A column plus what the DDL needs that `Column` does not carry.
pub(crate) struct ColumnRow {
    pub column: Column,
    /// `'s'` stored or `'v'` virtual generated column; empty otherwise.
    pub generated: String,
    /// `'a'` always or `'d'` by default identity; empty otherwise.
    pub identity: String,
}

const COLUMNS: &str = "\
    SELECT a.attname::text, format_type(a.atttypid, a.atttypmod), \
           CASE WHEN t.typtype = 'd' THEN t.typbasetype ELSE t.oid END, t.typcategory::text, \
           a.attnotnull, pg_get_expr(d.adbin, d.adrelid), col_description(a.attrelid, a.attnum), \
           a.attgenerated::text, a.attidentity::text \
    FROM pg_attribute a \
    JOIN pg_type t ON t.oid = a.atttypid \
    LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
    WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped \
    ORDER BY a.attnum";

pub(crate) async fn columns(client: &Client, oid: Oid) -> Result<Vec<ColumnRow>, DbError> {
    let rows = query(client, COLUMNS, &[&oid]).await?;
    rows.iter()
        .zip(1u32..)
        .map(|(row, ordinal)| {
            let generated: String = get(row, 7)?;
            let identity: String = get(row, 8)?;
            let default: Option<String> = get(row, 5)?;
            Ok(ColumnRow {
                column: Column {
                    name: get(row, 0)?,
                    type_name: get(row, 1)?,
                    category: type_category(get(row, 2)?, &get::<String>(row, 3)?),
                    nullable: !get::<bool>(row, 4)?,
                    default: default.or_else(|| identity_default(&identity)),
                    comment: get(row, 6)?,
                    ordinal,
                    generated: !generated.is_empty(),
                },
                generated,
                identity,
            })
        })
        .collect()
}

fn identity_default(identity: &str) -> Option<String> {
    match identity {
        "a" => Some("GENERATED ALWAYS AS IDENTITY".to_owned()),
        "d" => Some("GENERATED BY DEFAULT AS IDENTITY".to_owned()),
        _ => None,
    }
}

/// Built-in types (and domains over them) by name; others by `pg_type.typcategory`.
fn type_category(oid: Oid, typcategory: &str) -> TypeCategory {
    if let Some(ty) = Type::from_oid(oid) {
        return crate::decode::category(&ty);
    }
    match typcategory {
        "A" => TypeCategory::Array,
        "B" => TypeCategory::Bool,
        "D" => TypeCategory::Temporal,
        "E" | "S" => TypeCategory::Text,
        _ => TypeCategory::Other,
    }
}

pub(crate) struct ConstraintRow {
    pub name: String,
    pub kind: String,
    pub columns: Vec<String>,
    pub referenced_schema: Option<String>,
    pub referenced_table: Option<String>,
    pub referenced_columns: Vec<String>,
    pub on_update: String,
    pub on_delete: String,
    pub check: Option<String>,
    /// `pg_get_constraintdef`, for the DDL.
    pub definition: String,
}

const CONSTRAINTS: &str = "\
    SELECT con.conname::text, con.contype::text, \
           ARRAY(SELECT a.attname::text FROM unnest(con.conkey) WITH ORDINALITY k(n, i) \
                 JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.n ORDER BY k.i), \
           fn.nspname::text, fc.relname::text, \
           ARRAY(SELECT a.attname::text FROM unnest(con.confkey) WITH ORDINALITY k(n, i) \
                 JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.n ORDER BY k.i), \
           con.confupdtype::text, con.confdeltype::text, pg_get_expr(con.conbin, con.conrelid, true), \
           pg_get_constraintdef(con.oid, true) \
    FROM pg_constraint con \
    LEFT JOIN pg_class fc ON fc.oid = con.confrelid \
    LEFT JOIN pg_namespace fn ON fn.oid = fc.relnamespace \
    WHERE con.conrelid = $1 AND con.contype IN ('p', 'u', 'f', 'c', 'x') \
    ORDER BY CASE con.contype WHEN 'p' THEN 0 WHEN 'u' THEN 1 WHEN 'c' THEN 2 ELSE 3 END, con.conname";

pub(crate) async fn constraints(client: &Client, oid: Oid) -> Result<Vec<ConstraintRow>, DbError> {
    let rows = query(client, CONSTRAINTS, &[&oid]).await?;
    rows.iter()
        .map(|row| {
            Ok(ConstraintRow {
                name: get(row, 0)?,
                kind: get(row, 1)?,
                columns: get(row, 2)?,
                referenced_schema: get(row, 3)?,
                referenced_table: get(row, 4)?,
                referenced_columns: get(row, 5)?,
                on_update: get(row, 6)?,
                on_delete: get(row, 7)?,
                check: get(row, 8)?,
                definition: get(row, 9)?,
            })
        })
        .collect()
}

pub(crate) struct IndexRow {
    pub index: Index,
    /// `pg_get_indexdef`, for the DDL.
    pub definition: String,
}

/// Indexes that do not back a primary key, unique or exclusion constraint.
const INDEXES: &str = "\
    SELECT ic.relname::text, i.indisunique, am.amname::text, pg_get_expr(i.indpred, i.indrelid, true), \
           ARRAY(SELECT CASE WHEN i.indkey[k - 1] = 0 \
                             THEN pg_get_indexdef(i.indexrelid, k, true) \
                             ELSE (SELECT a.attname::text FROM pg_attribute a \
                                   WHERE a.attrelid = i.indrelid AND a.attnum = i.indkey[k - 1]) END \
                 FROM generate_series(1, i.indnkeyatts) k ORDER BY k), \
           pg_get_indexdef(i.indexrelid) \
    FROM pg_index i \
    JOIN pg_class ic ON ic.oid = i.indexrelid \
    JOIN pg_am am ON am.oid = ic.relam \
    WHERE i.indrelid = $1 AND NOT EXISTS ( \
        SELECT 1 FROM pg_constraint c \
        WHERE c.conindid = i.indexrelid AND c.conrelid = i.indrelid AND c.contype IN ('p', 'u', 'x')) \
    ORDER BY ic.relname";

pub(crate) async fn indexes(client: &Client, oid: Oid) -> Result<Vec<IndexRow>, DbError> {
    let rows = query(client, INDEXES, &[&oid]).await?;
    rows.iter()
        .map(|row| {
            Ok(IndexRow {
                index: Index {
                    name: get(row, 0)?,
                    unique: get(row, 1)?,
                    method: get(row, 2)?,
                    predicate: get(row, 3)?,
                    columns: get(row, 4)?,
                },
                definition: get(row, 5)?,
            })
        })
        .collect()
}

pub(crate) async fn query(
    client: &Client,
    sql: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<Vec<Row>, DbError> {
    client
        .query(sql, params)
        .await
        .map_err(|e| map_error(&e, ""))
}

pub(crate) fn get<'a, T: tokio_postgres::types::FromSql<'a>>(
    row: &'a Row,
    index: usize,
) -> Result<T, DbError> {
    row.try_get(index).map_err(|e| map_error(&e, ""))
}
