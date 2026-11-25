#![allow(clippy::unwrap_used)]

use rstest::rstest;

use super::*;

fn mysql() -> SplitterRules {
    SplitterRules {
        backtick_identifiers: true,
        hash_comments: true,
        backslash_escapes: true,
        ..SplitterRules::default()
    }
}

fn column(filter: &str) -> Option<String> {
    single_column(filter, &["o", "orders"], SplitterRules::default())
}

#[rstest]
#[case::postgres_equality("(total = 5)", "total")]
#[case::postgres_text_cast("((note)::text = 'x'::text)", "note")]
#[case::postgres_any("(total = ANY ('{1,2,3}'::integer[]))", "total")]
#[case::postgres_range("((total >= 5) AND (total < 10))", "total")]
#[case::postgres_parameter("(total = $1)", "total")]
#[case::mysql_with_derived_not_null("((o.total = 5) and (o.cid is not null))", "total")]
#[case::mariadb_plain("o.total = 5 and o.cid is not null", "total")]
#[case::qualified_by_table("orders.total > -3", "total")]
#[case::in_list("total in (1,2,3)", "total")]
#[case::between("total between 1 and 10", "total")]
#[case::is_null("note is null", "note")]
#[case::negative_decimal("total <= -1.5", "total")]
#[case::string("note = 'a and b'", "note")]
fn simple_filters_name_one_column(#[case] filter: &str, #[case] expected: &str) {
    assert_eq!(column(filter).as_deref(), Some(expected), "{filter}");
}

#[rstest]
#[case::two_columns("(total = 5) AND (cid = 3)")]
#[case::or("(total = 5) OR (total = 6)")]
#[case::function_on_column("(lower(note) = 'x')")]
#[case::explicit_cast("((total)::bigint = 5)")]
#[case::join_condition("(o.cid = c.id)")]
#[case::other_table("(c.total = 5)")]
#[case::not_equal("(total <> 5)")]
#[case::like("(note ~~ 'a%'::text)")]
#[case::only_not_null("(cid IS NOT NULL)")]
#[case::subquery("(total = (SelECT max(x) FROM t))")]
#[case::empty("")]
#[case::unbalanced("((total = 5)")]
fn anything_else_names_none(#[case] filter: &str) {
    assert_eq!(column(filter), None, "{filter}");
}

#[test]
fn backticks_are_identifiers_on_mysql() {
    let found = single_column("(`o`.`total` = 5)", &["o"], mysql());
    assert_eq!(found.as_deref(), Some("total"));
}

#[test]
fn qualifiers_compare_without_case() {
    assert_eq!(
        single_column("(O.total = 5)", &["o"], SplitterRules::default()).as_deref(),
        Some("total")
    );
}
