//! Validation of an edit, and the description the table should have after it.

use super::{ColumnDefinition, SchemaChange, SchemaEdit, check_change_sql, refused};
use crate::dialect::Dialect;
use crate::error::DbError;
use crate::result::TypeCategory;
use crate::schema::{Column, ForeignKey, Index, ObjectKind, TableInfo};

/// Checks every change against the table as the changes before it left it, and returns the
/// description the table should have afterwards. Shared by every driver; the SQLite planner
/// builds its table rebuild from it.
///
/// Drops match the object by value, so the frontend sends drops of keys, indexes and
/// constraints before the column changes that would alter them.
pub fn expected_table(dialect: &dyn Dialect, edit: &SchemaEdit) -> Result<TableInfo, DbError> {
    let mut states = table_states(dialect, edit)?;
    states.pop().ok_or_else(|| refused("Nothing to apply"))
}

/// The table before each change, then after the last one: `changes.len() + 1` descriptions,
/// validated as [`expected_table`] does. Planners read a change's starting point from it.
pub fn table_states(dialect: &dyn Dialect, edit: &SchemaEdit) -> Result<Vec<TableInfo>, DbError> {
    if edit.original.kind != ObjectKind::Table {
        return Err(refused("Only tables can be edited, not views"));
    }
    if edit.changes.is_empty() {
        return Err(refused("Nothing to apply"));
    }
    let mut states = vec![edit.original.clone()];
    for change in &edit.changes {
        check_change_sql(change, dialect)?;
        let before = states
            .last()
            .cloned()
            .unwrap_or_else(|| edit.original.clone());
        states.push(apply_change(before, change)?);
    }
    let last = states
        .last_mut()
        .ok_or_else(|| refused("Nothing to apply"))?;
    if last.columns.is_empty() {
        return Err(refused("A table needs at least one column"));
    }
    last.row_key = last.choose_row_key();
    Ok(states)
}

fn apply_change(mut info: TableInfo, change: &SchemaChange) -> Result<TableInfo, DbError> {
    match change {
        SchemaChange::AddColumn { column } => {
            check_name("column", &column.name)?;
            check_free_column(&info, &column.name)?;
            let ordinal = info.columns.iter().map(|c| c.ordinal).max().unwrap_or(0) + 1;
            info.columns
                .push(new_column(column, ordinal, TypeCategory::Other));
        }
        SchemaChange::AlterColumn { name, column, .. } => return alter_column(info, name, column),
        SchemaChange::DropColumn { name } => {
            position(&info, name)?;
            if let Some(user) = column_user(&info, name) {
                return Err(refused(format!(
                    "{name} is used by {user}; drop that first"
                )));
            }
            info.columns.retain(|c| &c.name != name);
        }
        SchemaChange::AddPrimaryKey { key } => {
            if info.primary_key.is_some() {
                return Err(refused(format!(
                    "{} already has a primary key",
                    info.table.name
                )));
            }
            check_key_columns(&info, &key.columns)?;
            info.primary_key = Some(key.clone());
        }
        SchemaChange::DropPrimaryKey => {
            if info.primary_key.take().is_none() {
                return Err(refused(format!("{} has no primary key", info.table.name)));
            }
        }
        SchemaChange::AddUnique { key } => {
            check_key_columns(&info, &key.columns)?;
            info.unique_constraints.push(key.clone());
        }
        SchemaChange::DropUnique { key } => {
            info.unique_constraints = remove(info.unique_constraints, key, "unique constraint")?;
        }
        SchemaChange::AddForeignKey { foreign_key } => {
            check_foreign_key(&info, foreign_key)?;
            info.foreign_keys.push(foreign_key.clone());
        }
        SchemaChange::DropForeignKey { foreign_key } => {
            info.foreign_keys = remove(info.foreign_keys, foreign_key, "foreign key")?;
        }
        SchemaChange::AddCheck { check } => info.check_constraints.push(check.clone()),
        SchemaChange::DropCheck { check } => {
            info.check_constraints = remove(info.check_constraints, check, "check constraint")?;
        }
        SchemaChange::AddIndex { index } => {
            check_index(&info, index)?;
            info.indexes.push(index.clone());
        }
        SchemaChange::DropIndex { index } => {
            info.indexes = remove(info.indexes, index, "index")?;
        }
        SchemaChange::RenameTable { name } => {
            check_name("table", name)?;
            let old = info.table.clone();
            info.table.name.clone_from(name);
            for fk in &mut info.foreign_keys {
                if fk.referenced_table == old {
                    fk.referenced_table = info.table.clone();
                }
            }
        }
        SchemaChange::SetComment { comment } => info.comment.clone_from(comment),
    }
    Ok(info)
}

fn new_column(definition: &ColumnDefinition, ordinal: u32, category: TypeCategory) -> Column {
    Column {
        name: definition.name.clone(),
        type_name: definition.type_name.clone(),
        category,
        nullable: definition.nullable,
        default: definition.default.clone(),
        comment: definition.comment.clone(),
        ordinal,
        generated: false,
    }
}

fn alter_column(
    mut info: TableInfo,
    name: &str,
    definition: &ColumnDefinition,
) -> Result<TableInfo, DbError> {
    let at = position(&info, name)?;
    let old = info.columns[at].clone();
    if old.generated {
        return Err(refused(format!(
            "{name} is a generated column: drop it and add it again to change it"
        )));
    }
    check_name("column", &definition.name)?;
    if definition.name != name {
        check_free_column(&info, &definition.name)?;
    }
    let category = if definition.type_name == old.type_name {
        old.category
    } else {
        TypeCategory::Other
    };
    info.columns[at] = new_column(definition, old.ordinal, category);
    if definition.name != name {
        info = rename_references(info, name, &definition.name);
    }
    Ok(info)
}

/// Follows a column rename into keys, indexes and foreign keys (and the referenced side of a
/// foreign key to this same table). Index expressions are left alone.
fn rename_references(mut info: TableInfo, old: &str, new: &str) -> TableInfo {
    let rename = |columns: &mut Vec<String>| {
        for column in columns.iter_mut().filter(|c| c.as_str() == old) {
            new.clone_into(column);
        }
    };
    if let Some(key) = &mut info.primary_key {
        rename(&mut key.columns);
    }
    for key in &mut info.unique_constraints {
        rename(&mut key.columns);
    }
    for index in &mut info.indexes {
        rename(&mut index.columns);
    }
    for fk in &mut info.foreign_keys {
        rename(&mut fk.columns);
        if fk.referenced_table == info.table {
            rename(&mut fk.referenced_columns);
        }
    }
    info
}

fn check_name(what: &str, name: &str) -> Result<(), DbError> {
    if name.trim().is_empty() {
        return Err(refused(format!("The {what} needs a name")));
    }
    Ok(())
}

fn position(info: &TableInfo, name: &str) -> Result<usize, DbError> {
    info.columns
        .iter()
        .position(|c| c.name == name)
        .ok_or_else(|| refused(format!("{} has no column {name}", info.table.name)))
}

fn check_free_column(info: &TableInfo, name: &str) -> Result<(), DbError> {
    if info.columns.iter().any(|c| c.name == name) {
        return Err(refused(format!(
            "{} already has a column {name}",
            info.table.name
        )));
    }
    Ok(())
}

/// What keeps a column from being dropped, if anything.
fn column_user(info: &TableInfo, name: &str) -> Option<String> {
    let uses = |columns: &[String]| columns.iter().any(|c| c == name);
    let label = |n: &Option<String>, kind: &str| n.clone().unwrap_or_else(|| kind.to_owned());
    if info.primary_key.as_ref().is_some_and(|k| uses(&k.columns)) {
        return Some("the primary key".into());
    }
    if let Some(key) = info.unique_constraints.iter().find(|k| uses(&k.columns)) {
        return Some(label(&key.name, "a unique constraint"));
    }
    if let Some(fk) = info.foreign_keys.iter().find(|fk| {
        uses(&fk.columns) || (fk.referenced_table == info.table && uses(&fk.referenced_columns))
    }) {
        return Some(label(&fk.name, "a foreign key"));
    }
    info.indexes
        .iter()
        .find(|i| uses(&i.columns))
        .map(|i| i.name.clone())
}

fn check_key_columns(info: &TableInfo, columns: &[String]) -> Result<(), DbError> {
    if columns.is_empty() {
        return Err(refused("A key needs at least one column"));
    }
    for (i, column) in columns.iter().enumerate() {
        position(info, column)?;
        if columns[..i].contains(column) {
            return Err(refused(format!("{column} is named twice in the key")));
        }
    }
    Ok(())
}

fn check_foreign_key(info: &TableInfo, fk: &ForeignKey) -> Result<(), DbError> {
    check_key_columns(info, &fk.columns)?;
    check_name("referenced table", &fk.referenced_table.name)?;
    if fk.columns.len() != fk.referenced_columns.len() {
        return Err(refused(
            "A foreign key needs the same number of columns on both sides",
        ));
    }
    Ok(())
}

fn check_index(info: &TableInfo, index: &Index) -> Result<(), DbError> {
    check_name("index", &index.name)?;
    if index.columns.is_empty() {
        return Err(refused("An index needs at least one column or expression"));
    }
    if let Some(method) = &index.method {
        let plain = !method.is_empty()
            && method
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !plain {
            return Err(refused(format!(
                "The index method must be a plain name, such as btree: {method}"
            )));
        }
    }
    if info.indexes.iter().any(|i| i.name == index.name) {
        return Err(refused(format!(
            "{} already has an index {}",
            info.table.name, index.name
        )));
    }
    Ok(())
}

/// The list without `item`, which must be in it.
fn remove<T: PartialEq>(items: Vec<T>, item: &T, what: &str) -> Result<Vec<T>, DbError> {
    let before = items.len();
    let kept: Vec<T> = items.into_iter().filter(|i| i != item).collect();
    if kept.len() == before {
        return Err(refused(format!("The table has no such {what}")));
    }
    Ok(kept)
}
