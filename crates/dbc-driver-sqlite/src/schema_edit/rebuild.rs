//! The table rebuild from the SQLite manual ("Making Other Kinds Of Table Schema Changes"),
//! D-042. The new `CREATE TABLE` is the stored text edited item by item, so what the model does
//! not know (collations, `AUTOINCREMENT`, conflict clauses, `WITHOUT ROWID`, `STRICT`) survives.
//!
//! The rebuild uses the columns' original names; renames run natively at the end, with
//! `legacy_alter_table` off, so SQLite rewrites indexes, triggers and views itself.

use std::collections::HashSet;

use dbc_core::DbError;
use dbc_core::dialect::{Dialect, string_literal};
use dbc_core::schema::{Column, ForeignKey, Index, Key, TableInfo, TableRef};
use dbc_core::schema_edit::render::{
    check_clause, default_clause, foreign_key_clause, primary_key_clause, unique_clause,
};
use dbc_core::schema_edit::{ColumnDefinition, SchemaChange, SchemaCheck, SchemaEdit, SchemaPlan};
use dbc_core::sql::definition::{
    Clause, ColumnParts, column_name, split_column, split_create_table,
};

use super::matching::{check_expression, clause_kind, constraint_columns, kind, normalise};
use super::{TableSource, column_sql, create_index, ident, refused};
use crate::SqliteDialect;

const TEMPORARY_PREFIX: &str = "_dunlin_rebuild_";

#[derive(Debug, Clone)]
enum Item {
    Column {
        /// The name in the rebuilt table: the original one, or a new column's own.
        rebuild_name: String,
        /// The name the edit knows it by at this point.
        current: String,
        original: bool,
        parts: ColumnParts,
    },
    Constraint(String),
}

#[derive(Default)]
struct Rebuild {
    /// The table's name before the edit, which it keeps until the end of the rebuild.
    table_name: String,
    items: Vec<Item>,
    added_indexes: Vec<Index>,
    dropped_indexes: HashSet<String>,
    rename_table: Option<String>,
}

pub(super) fn plan(
    edit: &SchemaEdit,
    states: &[TableInfo],
    source: &TableSource,
) -> Result<SchemaPlan, DbError> {
    let rules = SqliteDialect.splitter_rules();
    let parts = split_create_table(&source.create_table, rules)
        .ok_or_else(|| refused("Could not read the table's CREATE TABLE text"))?;
    let mut rebuild = Rebuild {
        table_name: edit.original.table.name.clone(),
        items: parts
            .items
            .iter()
            .map(
                |item| match (column_name(item, rules), split_column(item, rules)) {
                    (Some(name), Some(parts)) => Item::Column {
                        rebuild_name: name.clone(),
                        current: name,
                        original: true,
                        parts,
                    },
                    _ => Item::Constraint(item.clone()),
                },
            )
            .collect(),
        ..Rebuild::default()
    };
    for (change, before) in edit.changes.iter().zip(states) {
        rebuild.change(before, change)?;
    }
    rebuild.check_names()?;
    let original = &edit.original;
    let mut statements = rebuild.copy_statements(original, &parts.tail, source)?;
    statements.extend(rebuild.recreate_statements(original, source));
    let renames = rebuild.rename_statements(original);
    if !renames.is_empty() {
        statements.push("PRAGMA legacy_alter_table = OFF".into());
        statements.extend(renames);
    }
    Ok(SchemaPlan {
        before: settings(source, "OFF", "ON"),
        statements,
        after: settings(source, "ON", "OFF"),
        transactional: true,
        destructive: true,
        warnings: vec![
            "SQLite cannot make this change in place: the table is rebuilt (its rows are copied \
             into a new table that replaces it) and its indexes and triggers are recreated"
                .into(),
        ],
        checks: foreign_key_check(source, states),
    })
}

/// Foreign keys off and `legacy_alter_table` on during the rebuild, and back afterwards; only
/// what needs changing.
fn settings(source: &TableSource, foreign_keys: &str, legacy: &str) -> Vec<String> {
    let mut out = Vec::new();
    if source.foreign_keys {
        out.push(format!("PRAGMA foreign_keys = {foreign_keys}"));
    }
    if !source.legacy_alter_table {
        out.push(format!("PRAGMA legacy_alter_table = {legacy}"));
    }
    out
}

fn foreign_key_check(source: &TableSource, states: &[TableInfo]) -> Vec<SchemaCheck> {
    let Some(last) = states.last().filter(|_| source.foreign_keys) else {
        return Vec::new();
    };
    let schema = last.table.namespace.schema.as_deref().unwrap_or("main");
    vec![SchemaCheck {
        sql: format!(
            "PRAGMA {}.foreign_key_check({})",
            ident(schema),
            ident(&last.table.name)
        ),
        message: format!(
            "Some rows of {} (or of tables referencing it) no longer match a foreign key",
            last.table.name
        ),
    }]
}

impl Rebuild {
    fn column_at(&self, current: &str) -> Result<usize, DbError> {
        self.items
            .iter()
            .position(|i| matches!(i, Item::Column { current: c, .. } if c == current))
            .ok_or_else(|| {
                refused(format!(
                    "Could not find column {current} in the table's CREATE TABLE text"
                ))
            })
    }

    /// The rebuild name of a column the edit knows by `current`; other names pass through
    /// (index expressions, columns of other tables).
    fn rebuild_name(&self, current: &str) -> String {
        self.items
            .iter()
            .find_map(|i| match i {
                Item::Column {
                    rebuild_name,
                    current: c,
                    ..
                } if c == current => Some(rebuild_name.clone()),
                _ => None,
            })
            .unwrap_or_else(|| current.to_owned())
    }

    fn rebuild_names(&self, columns: &[String]) -> Vec<String> {
        columns.iter().map(|c| self.rebuild_name(c)).collect()
    }

    fn add_constraint(&mut self, text: String) {
        self.items.push(Item::Constraint(text));
    }

    fn change(&mut self, before: &TableInfo, change: &SchemaChange) -> Result<(), DbError> {
        let dialect = &SqliteDialect;
        match change {
            SchemaChange::AddColumn { column } => self.add_column(column)?,
            SchemaChange::AlterColumn { name, column, .. } => {
                let old = before
                    .column_named(name)
                    .ok_or_else(|| refused(format!("No column {name}")))?;
                self.alter_column(&old, column)?;
            }
            SchemaChange::DropColumn { name } => {
                let at = self.column_at(name)?;
                self.items.remove(at);
            }
            SchemaChange::AddPrimaryKey { key } => {
                let key = self.mapped_key(key);
                self.add_constraint(primary_key_clause(dialect, &key));
            }
            SchemaChange::DropPrimaryKey => self.drop_constraint("PRIMARY", |_| true, None)?,
            SchemaChange::AddUnique { key } => {
                let key = self.mapped_key(key);
                self.add_constraint(unique_clause(dialect, &key));
            }
            SchemaChange::DropUnique { key } => {
                let columns = self.rebuild_names(&key.columns);
                let single = (key.columns.len() == 1).then(|| key.columns[0].clone());
                self.drop_constraint("UNIQUE", |t| constraint_columns(t) == columns, single)?;
            }
            SchemaChange::AddForeignKey { foreign_key } => {
                let fk = self.mapped_foreign_key(before, foreign_key);
                self.add_constraint(foreign_key_clause(dialect, &fk));
            }
            SchemaChange::DropForeignKey { foreign_key } => {
                let columns = self.rebuild_names(&foreign_key.columns);
                let single =
                    (foreign_key.columns.len() == 1).then(|| foreign_key.columns[0].clone());
                self.drop_constraint("FOREIGN", |t| constraint_columns(t) == columns, single)?;
            }
            SchemaChange::AddCheck { check } => {
                self.add_constraint(check_clause(dialect, check));
            }
            SchemaChange::DropCheck { check } => {
                let wanted = normalise(&check.expression);
                let matches = |t: &str| check_expression(t).as_deref() == Some(wanted.as_str());
                self.drop_check(&matches)?;
            }
            SchemaChange::AddIndex { index } => self.added_indexes.push(Index {
                columns: self.rebuild_names(&index.columns),
                ..index.clone()
            }),
            SchemaChange::DropIndex { index } => {
                let before_len = self.added_indexes.len();
                self.added_indexes.retain(|i| i.name != index.name);
                if self.added_indexes.len() == before_len {
                    self.dropped_indexes.insert(index.name.clone());
                }
            }
            SchemaChange::RenameTable { name } => self.rename_table = Some(name.clone()),
            SchemaChange::SetComment { .. } => {
                return Err(refused("SQLite has no table comments"));
            }
        }
        Ok(())
    }

    fn mapped_key(&self, key: &Key) -> Key {
        Key {
            name: key.name.clone(),
            columns: self.rebuild_names(&key.columns),
        }
    }

    fn mapped_foreign_key(&self, before: &TableInfo, fk: &ForeignKey) -> ForeignKey {
        let to_self = fk.referenced_table == before.table;
        ForeignKey {
            columns: self.rebuild_names(&fk.columns),
            // SQLite resolves the parent table in the child's own schema, unqualified.
            referenced_table: TableRef {
                namespace: Default::default(),
                // The table keeps its old name until the very end of the rebuild.
                name: if to_self {
                    self.table_name.clone()
                } else {
                    fk.referenced_table.name.clone()
                },
            },
            referenced_columns: if to_self {
                self.rebuild_names(&fk.referenced_columns)
            } else {
                fk.referenced_columns.clone()
            },
            ..fk.clone()
        }
    }

    fn add_column(&mut self, column: &ColumnDefinition) -> Result<(), DbError> {
        let rules = SqliteDialect.splitter_rules();
        let parts = split_column(&column_sql(column), rules)
            .ok_or_else(|| refused(format!("Could not write column {}", column.name)))?;
        let after_last_column = self
            .items
            .iter()
            .rposition(|i| matches!(i, Item::Column { .. }))
            .map_or(0, |i| i + 1);
        self.items.insert(
            after_last_column,
            Item::Column {
                rebuild_name: column.name.clone(),
                current: column.name.clone(),
                original: false,
                parts,
            },
        );
        Ok(())
    }

    fn alter_column(&mut self, old: &Column, new: &ColumnDefinition) -> Result<(), DbError> {
        let at = self.column_at(&old.name)?;
        let Item::Column {
            rebuild_name,
            original,
            parts,
            ..
        } = self.items[at].clone()
        else {
            return Err(refused(format!("{} is not a column", old.name)));
        };
        let mut parts = changed_parts(parts, old, new);
        let rebuild_name = if original {
            rebuild_name
        } else {
            parts.name = ident(&new.name);
            new.name.clone()
        };
        self.items[at] = Item::Column {
            rebuild_name,
            current: new.name.clone(),
            original,
            parts,
        };
        Ok(())
    }

    /// Removes a table constraint of `wanted` kind that `matches` accepts, else, for a
    /// one-column constraint, the clause of that kind on the column.
    fn drop_constraint(
        &mut self,
        wanted: &str,
        matches: impl Fn(&str) -> bool,
        column: Option<String>,
    ) -> Result<(), DbError> {
        let found = self.items.iter().position(
            |i| matches!(i, Item::Constraint(text) if kind(text) == wanted && matches(text)),
        );
        if let Some(at) = found {
            self.items.remove(at);
            return Ok(());
        }
        let column_kind = if wanted == "FOREIGN" {
            "REFERENCES"
        } else {
            wanted
        };
        let removed = match column {
            Some(column) => self.remove_column_clause(column.as_str(), column_kind),
            None if wanted == "PRIMARY" => self.remove_any_column_clause("PRIMARY"),
            None => false,
        };
        if removed {
            return Ok(());
        }
        Err(refused(format!(
            "Could not find the {} constraint in the table's CREATE TABLE text",
            wanted.to_ascii_lowercase()
        )))
    }

    fn remove_column_clause(&mut self, current: &str, clause: &str) -> bool {
        let Ok(at) = self.column_at(current) else {
            return false;
        };
        remove_clause(&mut self.items[at], |c| clause_kind(c) == clause)
    }

    fn remove_any_column_clause(&mut self, clause: &str) -> bool {
        self.items
            .iter_mut()
            .any(|item| remove_clause(item, |c| clause_kind(c) == clause))
    }

    fn drop_check(&mut self, matches: &dyn Fn(&str) -> bool) -> Result<(), DbError> {
        let found = self.items.iter().position(
            |i| matches!(i, Item::Constraint(text) if kind(text) == "CHECK" && matches(text)),
        );
        if let Some(at) = found {
            self.items.remove(at);
            return Ok(());
        }
        let removed = self
            .items
            .iter_mut()
            .any(|item| remove_clause(item, |c| clause_kind(c) == "CHECK" && matches(&c.text)));
        if removed {
            return Ok(());
        }
        Err(refused(
            "Could not find the check constraint in the table's CREATE TABLE text",
        ))
    }

    /// Two columns may not end up with one name in the rebuilt table (a column renamed away
    /// and a new column given its old name).
    fn check_names(&self) -> Result<(), DbError> {
        let mut seen = HashSet::new();
        for item in &self.items {
            if let Item::Column { rebuild_name, .. } = item
                && !seen.insert(rebuild_name.to_lowercase())
            {
                return Err(refused(format!(
                    "A new column takes the old name of a renamed one ({rebuild_name}): apply the \
                     rename first, then add the column"
                )));
            }
        }
        Ok(())
    }

    fn create_table(&self, temporary: &str, tail: &str) -> String {
        let items: Vec<String> = self
            .items
            .iter()
            .map(|i| match i {
                Item::Column { parts, .. } => parts.to_sql(),
                Item::Constraint(text) => text.clone(),
            })
            .collect();
        format!(
            "CREATE TABLE {temporary} (\n  {}\n{tail}",
            items.join(",\n  ")
        )
    }

    /// Create the new table, copy the rows, keep the `AUTOINCREMENT` counter, swap the tables.
    fn copy_statements(
        &self,
        original: &TableInfo,
        tail: &str,
        source: &TableSource,
    ) -> Result<Vec<String>, DbError> {
        let old = SqliteDialect.qualified_name(&original.table);
        let temporary_name = format!("{TEMPORARY_PREFIX}{}", original.table.name);
        let temporary = SqliteDialect.qualified_name(&TableRef {
            namespace: original.table.namespace.clone(),
            name: temporary_name.clone(),
        });
        let copied: Vec<String> = self
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Column {
                    rebuild_name,
                    original: true,
                    parts,
                    ..
                } if !is_generated(parts) => Some(ident(rebuild_name)),
                _ => None,
            })
            .collect();
        if copied.is_empty() {
            return Err(refused(
                "Keep at least one of the table's existing columns, or its rows cannot be copied",
            ));
        }
        let create = self.create_table(&temporary, tail);
        let mut statements = vec![
            create.clone(),
            format!(
                "INSERT INTO {temporary} ({0}) SELECT {0} FROM {old}",
                copied.join(", ")
            ),
        ];
        if has_autoincrement(&source.create_table) && has_autoincrement(&create) {
            let schema = original.table.namespace.schema.as_deref().unwrap_or("main");
            let sequence = format!("{}.sqlite_sequence", ident(schema));
            let (new, old_name) = (
                string_literal(&temporary_name),
                string_literal(&original.table.name),
            );
            statements.push(format!("DELETE FROM {sequence} WHERE name = {new}"));
            statements.push(format!(
                "INSERT INTO {sequence} (name, seq) SELECT {new}, seq FROM {sequence} WHERE name = {old_name}"
            ));
        }
        statements.push(format!("DROP TABLE {old}"));
        statements.push(format!(
            "ALTER TABLE {temporary} RENAME TO {}",
            ident(&original.table.name)
        ));
        Ok(statements)
    }

    /// The table's indexes and triggers, as stored, then the new indexes.
    fn recreate_statements(&self, original: &TableInfo, source: &TableSource) -> Vec<String> {
        let kept = source
            .indexes
            .iter()
            .filter(|(name, _)| !self.dropped_indexes.contains(name))
            .map(|(_, sql)| sql.clone());
        let added = self
            .added_indexes
            .iter()
            .map(|index| create_index(original, &original.table.name, index));
        kept.chain(added)
            .chain(source.triggers.iter().cloned())
            .collect()
    }

    fn rename_statements(&self, original: &TableInfo) -> Vec<String> {
        let table = SqliteDialect.qualified_name(&original.table);
        let mut out: Vec<String> = self
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Column {
                    rebuild_name,
                    current,
                    original: true,
                    ..
                } if rebuild_name != current => Some(format!(
                    "ALTER TABLE {table} RENAME COLUMN {} TO {}",
                    ident(rebuild_name),
                    ident(current)
                )),
                _ => None,
            })
            .collect();
        if let Some(name) = &self.rename_table {
            out.push(format!("ALTER TABLE {table} RENAME TO {}", ident(name)));
        }
        out
    }
}

fn remove_clause(item: &mut Item, matches: impl Fn(&Clause) -> bool) -> bool {
    let Item::Column { parts, .. } = item else {
        return false;
    };
    let before = parts.clauses.len();
    parts.clauses.retain(|c| !matches(c));
    parts.clauses.len() != before
}

fn is_generated(parts: &ColumnParts) -> bool {
    parts
        .clauses
        .iter()
        .any(|c| matches!(clause_kind(c).as_str(), "GENERATED" | "AS"))
}

fn has_autoincrement(sql: &str) -> bool {
    sql.to_ascii_uppercase().contains("AUTOINCREMENT")
}

/// The column's stored definition with only what the user changed replaced.
fn changed_parts(mut parts: ColumnParts, old: &Column, new: &ColumnDefinition) -> ColumnParts {
    if new.type_name != old.type_name {
        parts.type_text.clone_from(&new.type_name);
    }
    if new.nullable != old.nullable {
        parts
            .clauses
            .retain(|c| !matches!(clause_kind(c).as_str(), "NOT" | "NULL"));
        if !new.nullable {
            parts.clauses.insert(
                0,
                Clause {
                    keyword: "NOT".into(),
                    text: "NOT NULL".into(),
                },
            );
        }
    }
    if new.default != old.default {
        let at = parts
            .clauses
            .iter()
            .position(|c| clause_kind(c) == "DEFAULT");
        let clause = new.default.as_deref().map(|d| Clause {
            keyword: "DEFAULT".into(),
            text: default_clause(d),
        });
        match (at, clause) {
            (Some(at), Some(clause)) => parts.clauses[at] = clause,
            (Some(at), None) => {
                parts.clauses.remove(at);
            }
            (None, Some(clause)) => parts.clauses.push(clause),
            (None, None) => {}
        }
    }
    parts
}
