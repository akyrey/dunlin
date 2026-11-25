#![allow(clippy::unwrap_used)]

use proptest::prelude::*;

use super::*;

fn mysql() -> SplitterRules {
    SplitterRules {
        backtick_identifiers: true,
        hash_comments: true,
        backslash_escapes: true,
        ..SplitterRules::default()
    }
}

fn sqlite() -> SplitterRules {
    SplitterRules {
        backtick_identifiers: true,
        bracket_identifiers: true,
        ..SplitterRules::default()
    }
}

const MYSQL_ORDERS: &str = "CREATE TABLE `orders` (
  `id` int NOT NULL AUTO_INCREMENT,
  `note` varchar(100) CHARACTER SET latin1 COLLATE latin1_bin DEFAULT 'a, (b)' COMMENT 'it''s, here',
  `updated` timestamp(6) NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
  `total` decimal(12,2) unsigned DEFAULT NULL,
  `full` varchar(10) GENERATED ALWAYS AS (concat(`note`,_utf8mb4' ')) VIRTUAL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `orders_note_key` (`note`),
  KEY `orders_total_idx` (`total`),
  CONSTRAINT `orders_fk` FOREIGN KEY (`total`) REFERENCES `t` (`id`) ON DELETE CASCADE,
  CONSTRAINT `orders_check` CHECK ((`total` >= 0))
) ENGINE=InnoDB AUTO_INCREMENT=3 DEFAULT CHARSET=utf8mb4 COMMENT='x'";

fn texts(clauses: &[Clause]) -> Vec<&str> {
    clauses.iter().map(|c| c.text.as_str()).collect()
}

#[test]
fn a_create_table_splits_at_top_level_commas() {
    let parts = split_create_table(MYSQL_ORDERS, mysql()).unwrap();
    assert_eq!(parts.head, "CREATE TABLE `orders` (");
    assert_eq!(
        parts.tail,
        ") ENGINE=InnoDB AUTO_INCREMENT=3 DEFAULT CHARSET=utf8mb4 COMMENT='x'"
    );
    assert_eq!(parts.items.len(), 10);
    assert_eq!(parts.items[0], "`id` int NOT NULL AUTO_INCREMENT");
    assert_eq!(
        parts.items[3],
        "`total` decimal(12,2) unsigned DEFAULT NULL"
    );
    assert_eq!(
        parts.items[9],
        "CONSTRAINT `orders_check` CHECK ((`total` >= 0))"
    );
}

#[test]
fn items_are_columns_or_constraints() {
    let parts = split_create_table(MYSQL_ORDERS, mysql()).unwrap();
    let kinds: Vec<Option<String>> = parts
        .items
        .iter()
        .map(|item| column_name(item, mysql()))
        .collect();
    assert_eq!(
        kinds[..5],
        [
            Some("id".into()),
            Some("note".into()),
            Some("updated".into()),
            Some("total".into()),
            Some("full".into())
        ]
    );
    assert!(kinds[5..].iter().all(Option::is_none), "{kinds:?}");
}

#[test]
fn quoted_names_are_unquoted() {
    assert_eq!(
        column_name("\"a \"\"b\"\"\" int", sqlite()),
        Some("a \"b\"".into())
    );
    assert_eq!(column_name("[a b] int", sqlite()), Some("a b".into()));
    assert_eq!(column_name("`a``b` int", mysql()), Some("a`b".into()));
    assert_eq!(column_name("plain", sqlite()), Some("plain".into()));
    assert_eq!(column_name("constraint c check (1)", sqlite()), None);
}

#[test]
fn a_column_splits_into_name_type_and_clauses() {
    let parts = split_create_table(MYSQL_ORDERS, mysql()).unwrap();
    let note = split_column(&parts.items[1], mysql()).unwrap();
    assert_eq!(note.name, "`note`");
    assert_eq!(note.type_text, "varchar(100)");
    assert_eq!(
        texts(&note.clauses),
        [
            "CHARACTER SET latin1",
            "COLLATE latin1_bin",
            "DEFAULT 'a, (b)'",
            "COMMENT 'it''s, here'"
        ]
    );
    let kinds: Vec<&str> = note.clauses.iter().map(|c| c.keyword.as_str()).collect();
    assert_eq!(kinds, ["CHARACTER", "COLLATE", "DEFAULT", "COMMENT"]);
}

#[test]
fn defaults_and_on_update_take_one_value() {
    let parts = split_create_table(MYSQL_ORDERS, mysql()).unwrap();
    let updated = split_column(&parts.items[2], mysql()).unwrap();
    assert_eq!(updated.type_text, "timestamp(6)");
    assert_eq!(
        texts(&updated.clauses),
        [
            "NULL",
            "DEFAULT CURRENT_TIMESTAMP(6)",
            "ON UPDATE CURRENT_TIMESTAMP(6)"
        ]
    );
    let total = split_column(&parts.items[3], mysql()).unwrap();
    assert_eq!(total.type_text, "decimal(12,2) unsigned");
    assert_eq!(texts(&total.clauses), ["DEFAULT NULL"]);
    let id = split_column(&parts.items[0], mysql()).unwrap();
    assert_eq!(texts(&id.clauses), ["NOT NULL", "AUTO_INCREMENT"]);
}

#[test]
fn signed_and_parenthesised_defaults() {
    let column = split_column("a int DEFAULT -1 NOT NULL", sqlite()).unwrap();
    assert_eq!(texts(&column.clauses), ["DEFAULT -1", "NOT NULL"]);
    let column = split_column("a int DEFAULT (1 + 2) CHECK (a > 0)", sqlite()).unwrap();
    assert_eq!(texts(&column.clauses), ["DEFAULT (1 + 2)", "CHECK (a > 0)"]);
}

#[test]
fn generated_columns_keep_their_expression_together() {
    let parts = split_create_table(MYSQL_ORDERS, mysql()).unwrap();
    let full = split_column(&parts.items[4], mysql()).unwrap();
    assert_eq!(
        texts(&full.clauses),
        ["GENERATED ALWAYS AS (concat(`note`,_utf8mb4' ')) VIRTUAL"]
    );
    let short = split_column("b int AS (a * 2) STORED", sqlite()).unwrap();
    assert_eq!(texts(&short.clauses), ["AS (a * 2) STORED"]);
}

#[test]
fn sqlite_column_constraints_keep_their_tails() {
    let column = split_column(
        "id INTEGER CONSTRAINT pk PRIMARY KEY DESC ON CONFLICT REPLACE AUTOINCREMENT \
         NOT NULL ON CONFLICT ABORT COLLATE NOCASE \
         REFERENCES parent(id) ON DELETE CASCADE NOT DEFERRABLE INITIALLY DEFERRED",
        sqlite(),
    )
    .unwrap();
    assert_eq!(column.type_text, "INTEGER");
    assert_eq!(
        texts(&column.clauses),
        [
            "CONSTRAINT pk PRIMARY KEY DESC ON CONFLICT REPLACE AUTOINCREMENT",
            "NOT NULL ON CONFLICT ABORT",
            "COLLATE NOCASE",
            "REFERENCES parent(id) ON DELETE CASCADE NOT DEFERRABLE INITIALLY DEFERRED"
        ]
    );
}

#[test]
fn a_column_may_have_no_type() {
    let column = split_column("a NOT NULL", sqlite()).unwrap();
    assert_eq!(column.type_text, "");
    assert_eq!(texts(&column.clauses), ["NOT NULL"]);
    let bare = split_column("a", sqlite()).unwrap();
    assert_eq!(bare.type_text, "");
    assert!(bare.clauses.is_empty());
}

#[test]
fn multi_word_types_stay_whole() {
    let column = split_column("a double precision NOT NULL, b", SplitterRules::default());
    assert!(column.is_none(), "a column is one item");
    let column = split_column("a double precision NOT NULL", SplitterRules::default()).unwrap();
    assert_eq!(column.type_text, "double precision");
}

#[test]
fn columns_join_back_into_one_definition() {
    let column = ColumnParts {
        name: "`a`".into(),
        type_text: "int".into(),
        clauses: vec![
            Clause {
                keyword: "NOT".into(),
                text: "NOT NULL".into(),
            },
            Clause {
                keyword: "COMMENT".into(),
                text: "COMMENT 'x'".into(),
            },
        ],
    };
    assert_eq!(column.to_sql(), "`a` int NOT NULL COMMENT 'x'");
    let untyped = ColumnParts {
        type_text: String::new(),
        clauses: vec![],
        ..column
    };
    assert_eq!(untyped.to_sql(), "`a`");
}

#[test]
fn comments_and_odd_spacing_are_tolerated() {
    let text = "create table if not exists \"t\"(-- first\n a int, /* b */ b text\n)";
    let parts = split_create_table(text, sqlite()).unwrap();
    assert_eq!(parts.head, "create table if not exists \"t\"(");
    assert_eq!(parts.items, ["-- first\n a int", "/* b */ b text"]);
    assert_eq!(parts.tail, ")");
    assert_eq!(column_name(&parts.items[0], sqlite()), Some("a".into()));
}

#[test]
fn text_that_is_not_a_create_table_is_none() {
    assert!(split_create_table("CREATE VIEW v AS SELECT 1", sqlite()).is_none());
    assert!(split_create_table("CREATE TABLE t AS SELECT 1", sqlite()).is_none());
    assert!(split_create_table("CREATE TABLE t (a int", sqlite()).is_none());
}

#[test]
fn parts_join_back_into_the_statement() {
    let parts = split_create_table(MYSQL_ORDERS, mysql()).unwrap();
    let rebuilt = parts.to_sql(",\n  ");
    assert_eq!(
        rebuilt.replace(['\n', ' '], ""),
        MYSQL_ORDERS.replace(['\n', ' '], "")
    );
}

proptest! {
    #[test]
    fn splitting_never_loses_text(
        columns in prop::collection::vec("[a-z]{1,8} (int|text|varchar\\(10\\))( NOT NULL)?( DEFAULT '[a-z,()]{0,5}')?", 1..6)
    ) {
        let text = format!("CREATE TABLE t ({})", columns.join(", "));
        let parts = split_create_table(&text, sqlite()).unwrap();
        prop_assert_eq!(&parts.items, &columns);
        prop_assert_eq!(parts.to_sql(", "), text);
        for column in &columns {
            let split = split_column(column, sqlite()).unwrap();
            prop_assert_eq!(split.to_sql(), column.clone());
        }
    }
}
