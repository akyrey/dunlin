//! Schema model returned by introspection (DRIVER_API.md).

use serde::{Deserialize, Serialize};

use crate::result::TypeCategory;

/// Where a table lives. See DRIVER_API.md for how each database maps onto it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct NamespaceRef {
    pub catalog: Option<String>,
    pub schema: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TableRef {
    pub namespace: NamespaceRef,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Namespace {
    pub namespace: NamespaceRef,
    pub name: String,
    /// Catalog schemas (`pg_catalog`, `information_schema`, `mysql`, ...), hidden by default.
    pub system: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Table,
    View,
    MaterializedView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ObjectSummary {
    pub name: String,
    pub kind: ObjectKind,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    pub type_name: String,
    pub category: TypeCategory,
    pub nullable: bool,
    pub default: Option<String>,
    pub comment: Option<String>,
    pub ordinal: u32,
    /// A generated (computed) column; `default` then holds its expression.
    pub generated: bool,
}

/// A primary key or unique constraint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Key {
    pub name: Option<String>,
    pub columns: Vec<String>,
}

/// Exported to TypeScript as `TableIndex`: `Index.ts` would clash with the generated
/// `index.ts` on case-insensitive file systems.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "TableIndex"))]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub name: String,
    /// Column names, or the expression text for an expression key part.
    pub columns: Vec<String>,
    pub unique: bool,
    pub method: Option<String>,
    pub predicate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ForeignKey {
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub referenced_table: TableRef,
    pub referenced_columns: Vec<String>,
    /// `None` is `NO ACTION`, the SQL default.
    pub on_update: Option<ReferentialAction>,
    pub on_delete: Option<ReferentialAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ReferentialAction {
    Cascade,
    SetNull,
    SetDefault,
    Restrict,
}

impl ReferentialAction {
    /// Parses the SQL spelling (`"SET NULL"`, `"cascade"`, ...). `NO ACTION` and unknown
    /// spellings are `None`.
    pub fn parse(sql: &str) -> Option<Self> {
        match sql.trim().to_ascii_uppercase().as_str() {
            "CASCADE" => Some(Self::Cascade),
            "SET NULL" => Some(Self::SetNull),
            "SET DEFAULT" => Some(Self::SetDefault),
            "RESTRICT" => Some(Self::Restrict),
            _ => None,
        }
    }

    pub fn as_sql(self) -> &'static str {
        match self {
            Self::Cascade => "CASCADE",
            Self::SetNull => "SET NULL",
            Self::SetDefault => "SET DEFAULT",
            Self::Restrict => "RESTRICT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub name: Option<String>,
    pub expression: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub table: TableRef,
    pub kind: ObjectKind,
    pub comment: Option<String>,
    pub columns: Vec<Column>,
    pub primary_key: Option<Key>,
    pub indexes: Vec<Index>,
    pub foreign_keys: Vec<ForeignKey>,
    pub unique_constraints: Vec<Key>,
    pub check_constraints: Vec<Check>,
    /// Columns used to address a row for editing; `None` means the table is read-only.
    pub row_key: Option<Vec<String>>,
}

impl TableInfo {
    /// The columns that address one row: the primary key, else the first unique constraint
    /// (then plain unique index without a predicate) whose columns are all `NOT NULL`.
    pub fn choose_row_key(&self) -> Option<Vec<String>> {
        if let Some(key) = &self.primary_key {
            return Some(key.columns.clone());
        }
        let not_null = |columns: &[String]| {
            !columns.is_empty()
                && columns
                    .iter()
                    .all(|name| self.columns.iter().any(|c| &c.name == name && !c.nullable))
        };
        let from_constraints = self
            .unique_constraints
            .iter()
            .map(|key| &key.columns)
            .find(|columns| not_null(columns));
        let from_indexes = || {
            self.indexes
                .iter()
                .filter(|index| index.unique && index.predicate.is_none())
                .map(|index| &index.columns)
                .find(|columns| not_null(columns))
        };
        from_constraints.or_else(from_indexes).cloned()
    }
}

/// Schema → table → column names for editor autocomplete.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CompletionSchema {
    pub tables: Vec<CompletionTable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CompletionTable {
    pub schema: Option<String>,
    pub name: String,
    pub columns: Vec<String>,
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn column(name: &str, nullable: bool) -> Column {
        Column {
            name: name.into(),
            type_name: "integer".into(),
            category: TypeCategory::Integer,
            nullable,
            default: None,
            comment: None,
            ordinal: 1,
            generated: false,
        }
    }

    fn key(columns: &[&str]) -> Key {
        Key {
            name: None,
            columns: columns.iter().map(|c| (*c).to_owned()).collect(),
        }
    }

    fn index(columns: &[&str], unique: bool, predicate: Option<&str>) -> Index {
        Index {
            name: "i".into(),
            columns: columns.iter().map(|c| (*c).to_owned()).collect(),
            unique,
            method: None,
            predicate: predicate.map(str::to_owned),
        }
    }

    fn table(columns: Vec<Column>) -> TableInfo {
        TableInfo {
            table: TableRef {
                namespace: NamespaceRef::default(),
                name: "t".into(),
            },
            kind: ObjectKind::Table,
            comment: None,
            columns,
            primary_key: None,
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            unique_constraints: Vec::new(),
            check_constraints: Vec::new(),
            row_key: None,
        }
    }

    #[test]
    fn row_key_prefers_the_primary_key() {
        let info = TableInfo {
            primary_key: Some(key(&["a", "b"])),
            unique_constraints: vec![key(&["c"])],
            ..table(vec![
                column("a", false),
                column("b", false),
                column("c", false),
            ])
        };

        assert_eq!(info.choose_row_key(), Some(vec!["a".into(), "b".into()]));
    }

    #[test]
    fn row_key_falls_back_to_the_first_not_null_unique_constraint() {
        let info = TableInfo {
            unique_constraints: vec![key(&["a"]), key(&["b", "c"])],
            ..table(vec![
                column("a", true),
                column("b", false),
                column("c", false),
            ])
        };

        assert_eq!(info.choose_row_key(), Some(vec!["b".into(), "c".into()]));
    }

    #[test]
    fn row_key_uses_a_plain_unique_index_after_constraints() {
        let info = TableInfo {
            indexes: vec![
                index(&["a"], false, None),
                index(&["a"], true, Some("a > 0")),
                index(&["lower(a)"], true, None),
                index(&["a"], true, None),
            ],
            ..table(vec![column("a", false)])
        };

        assert_eq!(info.choose_row_key(), Some(vec!["a".into()]));
    }

    #[test]
    fn row_key_is_none_without_a_usable_key() {
        let info = TableInfo {
            unique_constraints: vec![key(&["a"])],
            ..table(vec![column("a", true)])
        };

        assert_eq!(info.choose_row_key(), None);
    }

    #[rstest]
    #[case("CASCADE", Some(ReferentialAction::Cascade))]
    #[case("set null", Some(ReferentialAction::SetNull))]
    #[case(" SET DEFAULT ", Some(ReferentialAction::SetDefault))]
    #[case("RESTRICT", Some(ReferentialAction::Restrict))]
    #[case("NO ACTION", None)]
    #[case("", None)]
    fn referential_actions_parse_their_sql_spelling(
        #[case] sql: &str,
        #[case] expected: Option<ReferentialAction>,
    ) {
        assert_eq!(ReferentialAction::parse(sql), expected);
        if let Some(action) = expected {
            assert_eq!(ReferentialAction::parse(action.as_sql()), Some(action));
        }
    }
}
