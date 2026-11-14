use proptest::prelude::*;
use rstest::rstest;

use super::*;

fn ansi() -> SplitterRules {
    SplitterRules::default()
}

fn postgres() -> SplitterRules {
    SplitterRules {
        dollar_quoting: true,
        nested_block_comments: true,
        ..SplitterRules::default()
    }
}

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

fn statements(text: &str, rules: SplitterRules) -> Vec<&str> {
    split(text, rules)
        .iter()
        .map(|r| &text[r.start..r.end])
        .collect()
}

#[rstest]
#[case::two_statements("SELECT 1; SELECT 2;", ansi(), &["SELECT 1", "SELECT 2"])]
#[case::trailing_without_semicolon("SELECT 1;\nSELECT 2", ansi(), &["SELECT 1", "SELECT 2"])]
#[case::semicolon_in_string("SELECT 'a;b'; SELECT 2", ansi(), &["SELECT 'a;b'", "SELECT 2"])]
#[case::doubled_quote("SELECT 'it''s;'; SELECT 2", ansi(), &["SELECT 'it''s;'", "SELECT 2"])]
#[case::backslash_escape(r"SELECT 'a\';b'; SELECT 2", mysql(), &[r"SELECT 'a\';b'", "SELECT 2"])]
#[case::backslash_is_literal_in_ansi(r"SELECT 'a\'; SELECT 2", ansi(), &[r"SELECT 'a\'", "SELECT 2"])]
#[case::quoted_identifier("SELECT \"a;b\" FROM t; SELECT 2", ansi(), &["SELECT \"a;b\" FROM t", "SELECT 2"])]
#[case::backticks("SELECT `a;b`; SELECT 2", mysql(), &["SELECT `a;b`", "SELECT 2"])]
#[case::brackets("SELECT [a;b]; SELECT 2", sqlite(), &["SELECT [a;b]", "SELECT 2"])]
#[case::line_comment("SELECT 1 -- a;b\n; SELECT 2", ansi(), &["SELECT 1 -- a;b", "SELECT 2"])]
#[case::hash_comment("SELECT 1 # a;b\n; SELECT 2", mysql(), &["SELECT 1 # a;b", "SELECT 2"])]
#[case::block_comment("SELECT /* a;b */ 1; SELECT 2", ansi(), &["SELECT /* a;b */ 1", "SELECT 2"])]
#[case::nested_block_comment(
    "SELECT /* a /* b; */ c; */ 1; SELECT 2",
    postgres(),
    &["SELECT /* a /* b; */ c; */ 1", "SELECT 2"]
)]
#[case::non_nested_block_comment(
    "SELECT /* a /* b */ 1; SELECT 2",
    mysql(),
    &["SELECT /* a /* b */ 1", "SELECT 2"]
)]
#[case::dollar_quote(
    "CREATE FUNCTION f() AS $$ BEGIN; END $$; SELECT 2",
    postgres(),
    &["CREATE FUNCTION f() AS $$ BEGIN; END $$", "SELECT 2"]
)]
#[case::tagged_dollar_quote(
    "SELECT $fn$ a; $$ b; $fn$; SELECT 2",
    postgres(),
    &["SELECT $fn$ a; $$ b; $fn$", "SELECT 2"]
)]
#[case::positional_parameter("SELECT $1; SELECT 2", postgres(), &["SELECT $1", "SELECT 2"])]
#[case::empty_statements(";;  ; SELECT 1;;", ansi(), &["SELECT 1"])]
#[case::only_whitespace("  \n\t ", ansi(), &[])]
#[case::trailing_comment_only("SELECT 1; -- done", ansi(), &["SELECT 1"])]
#[case::leading_comment_belongs_to_statement(
    "-- users\nSELECT 1;",
    ansi(),
    &["-- users\nSELECT 1"]
)]
#[case::sqlite_trigger_body(
    "CREATE TRIGGER t AFTER INSERT ON a BEGIN UPDATE b SET x = 1; DELETE FROM c; END; SELECT 2",
    sqlite(),
    &["CREATE TRIGGER t AFTER INSERT ON a BEGIN UPDATE b SET x = 1; DELETE FROM c; END", "SELECT 2"]
)]
#[case::case_inside_trigger_body(
    "CREATE TRIGGER t AFTER INSERT ON a BEGIN SELECT CASE WHEN 1 THEN 2 END; END; SELECT 3",
    sqlite(),
    &["CREATE TRIGGER t AFTER INSERT ON a BEGIN SELECT CASE WHEN 1 THEN 2 END; END", "SELECT 3"]
)]
#[case::begin_transaction_is_not_a_block("BEGIN; SELECT 1; COMMIT", ansi(), &["BEGIN", "SELECT 1", "COMMIT"])]
#[case::unicode("SELECT 'é;🌍'; SELECT 'ü'", ansi(), &["SELECT 'é;🌍'", "SELECT 'ü'"])]
fn splits_statements(#[case] text: &str, #[case] rules: SplitterRules, #[case] expected: &[&str]) {
    let actual = statements(text, rules);

    assert_eq!(actual, expected);
}

#[test]
fn ranges_are_byte_offsets_into_the_input() {
    let text = "  SELECT 1 ;\n  SELECT 2";

    let ranges = split(text, ansi());

    assert_eq!(
        ranges,
        vec![
            StatementRange { start: 2, end: 10 },
            StatementRange { start: 15, end: 23 }
        ]
    );
}

/// A fragment of statement text that contains no top-level `;`, plus whether it is code.
fn fragment() -> impl Strategy<Value = (String, bool)> {
    prop_oneof![
        "[a-zA-Z_][a-zA-Z0-9_]{0,6}".prop_map(|s| (s, true)),
        "[0-9]{1,4}".prop_map(|s| (s, true)),
        "[,.=+*<>]".prop_map(|s| (s, true)),
        "[^']{0,8}".prop_map(|s| (format!("'{}'", s), true)),
        "[a-z;' ]{0,6}".prop_map(|s| (format!("'{}'", s.replace('\'', "''")), true)),
        "[a-z; ]{0,6}".prop_map(|s| (format!("\"{s}\""), true)),
        "[a-z;' ]{0,6}".prop_map(|s| (format!("$q${s}$q$"), true)),
        "[a-z;' ]{0,6}".prop_map(|s| (format!("/*{s}*/"), false)),
        "[a-z;' ]{0,6}".prop_map(|s| (format!("--{s}\n"), false)),
        "[ \t\n]{1,3}".prop_map(|s| (s, false)),
    ]
}

/// A statement: fragments joined by spaces, guaranteed to contain at least one code fragment.
fn statement() -> impl Strategy<Value = String> {
    ("[a-zA-Z]{1,6}", proptest::collection::vec(fragment(), 0..6)).prop_map(|(head, rest)| {
        let mut parts = vec![head];
        parts.extend(rest.into_iter().map(|(text, _)| text));
        parts.join(" ")
    })
}

proptest! {
    #[test]
    fn splits_generated_scripts_into_their_statements(
        stmts in proptest::collection::vec(statement(), 1..5),
        separator in "[ \n]{0,2};[ \n;]{0,3}",
    ) {
        let text = stmts.join(&separator);

        let actual = statements(&text, postgres());

        let expected: Vec<&str> = stmts.iter().map(|s| s.trim()).collect();
        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn ranges_are_ordered_disjoint_and_on_char_boundaries(text in "\\PC{0,80}") {
        for rules in [ansi(), postgres(), mysql(), sqlite()] {
            let ranges = split(&text, rules);

            for range in &ranges {
                prop_assert!(range.start < range.end && range.end <= text.len());
                prop_assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
                prop_assert_eq!(text[range.start..range.end].trim(), &text[range.start..range.end]);
            }
            for pair in ranges.windows(2) {
                prop_assert!(pair[0].end < pair[1].start);
            }
        }
    }
}
