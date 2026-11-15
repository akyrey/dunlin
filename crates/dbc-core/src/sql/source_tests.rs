use rstest::rstest;

use super::*;

fn ansi() -> SplitterRules {
    SplitterRules::default()
}

fn mysql() -> SplitterRules {
    SplitterRules {
        backtick_identifiers: true,
        hash_comments: true,
        backslash_escapes: true,
        ..SplitterRules::default()
    }
}

fn ident(value: &str, quoted: bool) -> Ident {
    Ident {
        value: value.into(),
        quoted,
    }
}

#[test]
fn a_star_select_from_one_table_has_that_source() {
    let source = single_table_source("SELECT * FROM people", ansi()).unwrap();

    assert_eq!(source.schema, None);
    assert_eq!(source.table, ident("people", false));
    assert_eq!(source.columns, SourceColumns::All);
}

#[test]
fn named_columns_schema_alias_and_trailing_clauses_are_understood() {
    let sql = "select p.id, name, \"Balance\" from public.people as p \
               where id in (select id from orders, x) order by name limit 10 offset 5;";

    let source = single_table_source(sql, ansi()).unwrap();

    assert_eq!(source.schema, Some(ident("public", false)));
    assert_eq!(source.table, ident("people", false));
    assert_eq!(
        source.columns,
        SourceColumns::Named(vec![
            ident("id", false),
            ident("name", false),
            ident("Balance", true)
        ])
    );
}

#[test]
fn quoted_identifiers_are_unquoted() {
    let source = single_table_source("SELECT `a``b` FROM `my db`.`t`", mysql()).unwrap();

    assert_eq!(source.schema, Some(ident("my db", true)));
    assert_eq!(
        source.columns,
        SourceColumns::Named(vec![ident("a`b", true)])
    );
}

#[test]
fn a_star_with_its_table_prefix_is_all_columns() {
    let source = single_table_source("SELECT t.* FROM t", ansi()).unwrap();

    assert_eq!(source.columns, SourceColumns::All);
}

#[rstest]
#[case::join("SELECT * FROM a JOIN b ON a.id = b.id")]
#[case::comma_join("SELECT * FROM a, b")]
#[case::group("SELECT id FROM a GROUP BY id")]
#[case::distinct("SELECT DISTINCT id FROM a")]
#[case::union("SELECT id FROM a UNION SELECT id FROM b")]
#[case::expression("SELECT id + 1 FROM a")]
#[case::function("SELECT count(*) FROM a")]
#[case::alias("SELECT id AS key FROM a")]
#[case::bare_alias("SELECT id key FROM a")]
#[case::literal("SELECT 1 FROM a")]
#[case::star_and_column("SELECT *, id FROM a")]
#[case::subquery_source("SELECT * FROM (SELECT 1) AS s")]
#[case::table_function("SELECT * FROM generate_series(1, 3)")]
#[case::three_part("SELECT * FROM db.s.t")]
#[case::no_from("SELECT 1")]
#[case::cte("WITH x AS (SELECT 1) SELECT * FROM x")]
#[case::not_select("UPDATE a SET id = 1")]
#[case::two_statements("SELECT * FROM a; SELECT * FROM b")]
#[case::into("SELECT * INTO b FROM a")]
#[case::locking("SELECT * FROM a FOR UPDATE")]
#[case::window("SELECT id FROM a WINDOW w AS (ORDER BY id)")]
#[case::empty("")]
fn anything_else_has_no_single_table_source(#[case] sql: &str) {
    assert_eq!(single_table_source(sql, ansi()), None, "{sql}");
}

proptest::proptest! {
    #[test]
    fn no_text_panics(text in "\\PC{0,80}") {
        let _ = single_table_source(&text, ansi());
        let _ = single_table_source(&text, mysql());
    }
}
