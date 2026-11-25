//! Finds the constraint a drop names in the stored `CREATE TABLE` text. SQLite does not name
//! most constraints, so they are matched by kind and columns (checks by expression).

use dbc_core::sql::definition::{Clause, unquote};
use dbc_core::sql::lexer::{Token, TokenKind, tokenize};

use crate::SqliteDialect;
use dbc_core::dialect::Dialect;

/// What a constraint is about: `PRIMARY`, `UNIQUE`, `FOREIGN` (or `REFERENCES` on a column),
/// `CHECK`, `NOT`, `DEFAULT`, ...: its first keyword after an optional `CONSTRAINT name`.
pub(super) fn kind(text: &str) -> String {
    let tokens = significant(text);
    let words: Vec<String> = tokens
        .iter()
        .map(|t| t.text(text).to_ascii_uppercase())
        .collect();
    let at = if words.first().map(String::as_str) == Some("CONSTRAINT") {
        2
    } else {
        0
    };
    words.get(at).cloned().unwrap_or_default()
}

pub(super) fn clause_kind(clause: &Clause) -> String {
    kind(&clause.text)
}

fn significant(text: &str) -> Vec<Token> {
    tokenize(text, SqliteDialect.splitter_rules())
        .into_iter()
        .filter(|t| !t.is_trivia())
        .collect()
}

/// The text inside the first top-level parenthesised group, without the parentheses.
pub(super) fn first_group(text: &str) -> Option<&str> {
    let tokens = significant(text);
    let open = tokens.iter().position(|t| t.kind == TokenKind::OpenParen)?;
    let mut depth = 0usize;
    for token in &tokens[open..] {
        match token.kind {
            TokenKind::OpenParen => depth += 1,
            TokenKind::CloseParen => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[tokens[open].end..token.start]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The column names of a key list such as `"a" COLLATE NOCASE, b DESC`, unquoted.
pub(super) fn key_columns(list: &str) -> Vec<String> {
    let mut columns = Vec::new();
    let mut expect_name = true;
    let mut depth = 0usize;
    for token in significant(list) {
        match token.kind {
            TokenKind::OpenParen => depth += 1,
            TokenKind::CloseParen => depth = depth.saturating_sub(1),
            TokenKind::Other if depth == 0 && token.text(list) == "," => expect_name = true,
            TokenKind::Word | TokenKind::QuotedIdent if depth == 0 && expect_name => {
                columns.push(unquote(token.text(list)));
                expect_name = false;
            }
            _ => {}
        }
    }
    columns
}

/// The columns a table constraint covers (`PRIMARY KEY (a, b)`, `FOREIGN KEY (a) ...`).
pub(super) fn constraint_columns(text: &str) -> Vec<String> {
    first_group(text).map(key_columns).unwrap_or_default()
}

/// The expression of a `CHECK (...)`, for comparing with an introspected one.
pub(super) fn check_expression(text: &str) -> Option<String> {
    first_group(text).map(normalise)
}

/// Text compared without whitespace or letter case.
pub(super) fn normalise(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn kinds_skip_a_constraint_name() {
        assert_eq!(kind("CONSTRAINT \"my pk\" PRIMARY KEY (a)"), "PRIMARY");
        assert_eq!(kind("unique (a, b)"), "UNIQUE");
        assert_eq!(kind("FOREIGN KEY (a) REFERENCES p(id)"), "FOREIGN");
    }

    #[test]
    fn key_columns_ignore_collations_and_order() {
        assert_eq!(
            constraint_columns("PRIMARY KEY (\"a b\" COLLATE NOCASE, [c] DESC)"),
            ["a b", "c"]
        );
        assert_eq!(
            constraint_columns("FOREIGN KEY (x) REFERENCES p (id)"),
            ["x"]
        );
    }

    #[test]
    fn checks_compare_without_spacing() {
        assert_eq!(
            check_expression("CONSTRAINT c CHECK ( total >= 0 )").unwrap(),
            normalise("total>=0")
        );
        assert_eq!(
            check_expression("CHECK ((a > 0) AND b)").unwrap(),
            "(a>0)andb"
        );
    }
}
