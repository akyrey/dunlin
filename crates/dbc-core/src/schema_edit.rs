//! Schema edits (Phase 9, D-042): explicit changes to one table, planned per driver and run by
//! [`apply`].
//!
//! The frontend stages [`SchemaChange`]s against the [`TableInfo`] it showed the user.
//! [`expected_table`] validates them (shared by every driver) and returns the description the
//! table should have afterwards; `Connection::plan_schema_edit` turns them into a
//! [`SchemaPlan`], which is text: DDL takes no bound parameters (SECURITY.md#generated-sql).

mod apply;
mod expected;
pub mod render;

use serde::{Deserialize, Serialize};

use crate::dialect::Dialect;
use crate::error::{DbError, DbErrorCode};
use crate::schema::{Check, ForeignKey, Index, Key, TableInfo};
use crate::sql::SplitterRules;
use crate::sql::lexer::{TokenKind, tokenize};

pub use apply::{SchemaApplyFailure, SchemaApplyReport, apply, check_current};
pub use expected::{expected_table, table_states};

/// Changes to one table, in the order they apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SchemaEdit {
    /// The description the changes were made against.
    pub original: TableInfo,
    pub changes: Vec<SchemaChange>,
}

/// A column as the user defines it. Type and default are SQL text, used as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ColumnDefinition {
    pub name: String,
    pub type_name: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub comment: Option<String>,
}

/// One change. A drop names the original object as introspected: PostgreSQL and MySQL drop it
/// by name, SQLite matches it by value. Changing a key, constraint or index is a drop and an add.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SchemaChange {
    AddColumn {
        column: ColumnDefinition,
    },
    /// `column.name` differing from `name` renames it. `using` converts existing values when
    /// the type changes (PostgreSQL only).
    AlterColumn {
        name: String,
        column: ColumnDefinition,
        using: Option<String>,
    },
    DropColumn {
        name: String,
    },
    AddPrimaryKey {
        key: Key,
    },
    DropPrimaryKey,
    AddUnique {
        key: Key,
    },
    DropUnique {
        key: Key,
    },
    AddForeignKey {
        #[serde(rename = "foreignKey")]
        foreign_key: ForeignKey,
    },
    DropForeignKey {
        #[serde(rename = "foreignKey")]
        foreign_key: ForeignKey,
    },
    AddCheck {
        check: Check,
    },
    DropCheck {
        check: Check,
    },
    AddIndex {
        index: Index,
    },
    DropIndex {
        index: Index,
    },
    RenameTable {
        name: String,
    },
    SetComment {
        comment: Option<String>,
    },
}

impl SchemaChange {
    /// Drops something the database cannot get back: data, or a key, index or constraint.
    pub fn is_destructive(&self) -> bool {
        matches!(
            self,
            Self::DropColumn { .. }
                | Self::DropPrimaryKey
                | Self::DropUnique { .. }
                | Self::DropForeignKey { .. }
                | Self::DropCheck { .. }
                | Self::DropIndex { .. }
        )
    }
}

/// What a driver plans for an edit. [`apply`] runs `before`, then `statements` (in one
/// transaction when `transactional`, else one at a time, stopping at the first failure), then
/// `after`, whatever happened.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SchemaPlan {
    pub before: Vec<String>,
    pub statements: Vec<String>,
    pub after: Vec<String>,
    pub transactional: bool,
    /// Drops a column, key, index or constraint, or rebuilds the table: needs confirmation.
    pub destructive: bool,
    /// Shown in the preview.
    pub warnings: Vec<String>,
}

impl SchemaPlan {
    /// Every statement in the order it runs, transaction control left out: the preview.
    pub fn preview(&self) -> Vec<String> {
        self.before
            .iter()
            .chain(&self.statements)
            .chain(&self.after)
            .cloned()
            .collect()
    }
}

pub(crate) fn refused(message: impl Into<String>) -> DbError {
    DbError::new(DbErrorCode::Execution, message)
}

/// Refuses SQL the user typed (a type, default, expression or predicate) unless it is one
/// expression: no `;` and balanced parentheses outside strings, quotes and comments, so it cannot
/// end the clause it is put into.
pub fn check_user_sql(what: &str, text: &str, rules: SplitterRules) -> Result<(), DbError> {
    if text.trim().is_empty() {
        return Err(refused(format!("The {what} is empty")));
    }
    let mut depth: i32 = 0;
    for token in tokenize(text, rules) {
        match token.kind {
            TokenKind::Semicolon => depth = -1,
            TokenKind::OpenParen => depth += 1,
            TokenKind::CloseParen => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            break;
        }
    }
    if depth != 0 {
        return Err(refused(format!(
            "The {what} must be one expression, without ';' or unbalanced parentheses: {text}"
        )));
    }
    Ok(())
}

/// The dialect-independent checks of the user SQL in one change.
fn check_change_sql(change: &SchemaChange, dialect: &dyn Dialect) -> Result<(), DbError> {
    let rules = dialect.splitter_rules();
    let column = |c: &ColumnDefinition| -> Result<(), DbError> {
        check_user_sql("type", &c.type_name, rules)?;
        if let Some(default) = &c.default {
            check_user_sql("default", default, rules)?;
        }
        Ok(())
    };
    match change {
        SchemaChange::AddColumn { column: c } => column(c),
        SchemaChange::AlterColumn {
            column: c, using, ..
        } => {
            column(c)?;
            using
                .as_deref()
                .map_or(Ok(()), |u| check_user_sql("USING expression", u, rules))
        }
        SchemaChange::AddCheck { check } => {
            check_user_sql("check expression", &check.expression, rules)
        }
        SchemaChange::AddIndex { index } => {
            for part in &index.columns {
                check_user_sql("index expression", part, rules)?;
            }
            index
                .predicate
                .as_deref()
                .map_or(Ok(()), |p| check_user_sql("index predicate", p, rules))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
