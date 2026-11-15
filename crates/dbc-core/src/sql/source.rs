//! Whether a `SELECT` reads plain columns of exactly one table, so its result rows can be
//! edited like the table's own (deferred feature 1). A lexer-level check that refuses anything
//! it is not sure of: joins, grouping, set operations, expressions and aliases.

use super::SplitterRules;
use super::lexer::{Token, TokenKind, tokenize};

/// An identifier as written: `quoted` ones keep their case, others fold as the dialect does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub value: String,
    pub quoted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceColumns {
    /// `*` or `t.*`: every column, in table order.
    All,
    /// The listed columns, in result order; a name may repeat.
    Named(Vec<Ident>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTable {
    pub schema: Option<Ident>,
    pub table: Ident,
    pub columns: SourceColumns,
}

/// Words that end the source table and may follow it: anything else after the table or its
/// alias means the shape is not understood.
const TRAILING: &[&str] = &["WHERE", "ORDER", "LIMIT", "OFFSET", "FETCH"];
/// Words that, anywhere outside parentheses, change which rows or columns come back.
const SHAPE_CHANGING: &[&str] = &[
    "JOIN",
    "GROUP",
    "HAVING",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "WINDOW",
    "INTO",
    "FOR",
    "DISTINCT",
    "NATURAL",
    "CROSS",
    "LATERAL",
];

struct Words<'a> {
    source: &'a str,
    tokens: Vec<Token>,
}

impl Words<'_> {
    fn text(&self, index: usize) -> Option<&str> {
        self.tokens
            .get(index)
            .and_then(|t| self.source.get(t.start..t.end))
    }

    fn is(&self, index: usize, word: &str) -> bool {
        self.tokens
            .get(index)
            .is_some_and(|t| t.kind == TokenKind::Word)
            && self
                .text(index)
                .is_some_and(|t| t.eq_ignore_ascii_case(word))
    }

    fn is_char(&self, index: usize, c: &str) -> bool {
        self.tokens
            .get(index)
            .is_some_and(|t| t.kind == TokenKind::Other)
            && self.text(index) == Some(c)
    }

    fn ident(&self, index: usize) -> Option<Ident> {
        let token = self.tokens.get(index)?;
        let text = self.text(index)?;
        match token.kind {
            TokenKind::Word if text.starts_with(|c: char| c.is_alphabetic() || c == '_') => {
                Some(Ident {
                    value: text.to_owned(),
                    quoted: false,
                })
            }
            TokenKind::QuotedIdent => unquote(text).map(|value| Ident {
                value,
                quoted: true,
            }),
            _ => None,
        }
    }
}

/// `"a""b"` → `a"b`, `` `a``b` `` → ``a`b``, `[a]` → `a`. `None` when unterminated.
fn unquote(text: &str) -> Option<String> {
    let open = text.chars().next()?;
    let close = if open == '[' { ']' } else { open };
    let inner = text.strip_prefix(open)?.strip_suffix(close)?;
    if open == '[' {
        return Some(inner.to_owned());
    }
    let doubled: String = [close, close].iter().collect();
    Some(inner.replace(&doubled, &close.to_string()))
}

/// The table a single `SELECT` reads, if it reads plain columns of one table and nothing else.
pub fn single_table_source(statement: &str, rules: SplitterRules) -> Option<SourceTable> {
    let mut tokens: Vec<Token> = tokenize(statement, rules)
        .into_iter()
        .filter(|t| !t.is_trivia())
        .collect();
    while tokens
        .last()
        .is_some_and(|t| t.kind == TokenKind::Semicolon)
    {
        tokens.pop();
    }
    let words = Words {
        source: statement,
        tokens,
    };
    if !words.is(0, "SELECT") || !shape_is_plain(&words) {
        return None;
    }
    let from = (1..words.tokens.len()).find(|&i| words.is(i, "FROM"))?;
    let columns = select_list(&words, 1, from)?;
    let (schema, table, end) = table_name(&words, from + 1)?;
    let after_alias = skip_alias(&words, end);
    if after_alias < words.tokens.len() && !TRAILING.iter().any(|w| words.is(after_alias, w)) {
        return None;
    }
    Some(SourceTable {
        schema,
        table,
        columns,
    })
}

/// No semicolon inside, and no shape-changing word or comma-join outside parentheses.
fn shape_is_plain(words: &Words) -> bool {
    let mut depth = 0usize;
    let mut after_from = false;
    for (i, token) in words.tokens.iter().enumerate() {
        match token.kind {
            TokenKind::Semicolon => return false,
            TokenKind::OpenParen => depth += 1,
            TokenKind::CloseParen => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            TokenKind::Word if SHAPE_CHANGING.iter().any(|w| words.is(i, w)) => return false,
            TokenKind::Word if words.is(i, "FROM") => after_from = true,
            TokenKind::Word if TRAILING.iter().any(|w| words.is(i, w)) => after_from = false,
            _ if after_from && words.is_char(i, ",") => return false,
            _ => {}
        }
    }
    true
}

/// `*`, `t.*`, or a list of `col` / `t.col`.
fn select_list(words: &Words, start: usize, end: usize) -> Option<SourceColumns> {
    let items: Vec<(usize, usize)> = {
        let mut items = Vec::new();
        let mut from = start;
        for i in start..end {
            if words.is_char(i, ",") {
                items.push((from, i));
                from = i + 1;
            }
        }
        items.push((from, end));
        items
    };
    let star = |(a, b): (usize, usize)| {
        (b == a + 1 && words.is_char(a, "*"))
            || (b == a + 3
                && words.ident(a).is_some()
                && words.is_char(a + 1, ".")
                && words.is_char(a + 2, "*"))
    };
    if let [only] = items.as_slice()
        && star(*only)
    {
        return Some(SourceColumns::All);
    }
    let names = items
        .into_iter()
        .map(|(a, b)| match b.checked_sub(a)? {
            1 => words.ident(a),
            3 if words.ident(a).is_some() && words.is_char(a + 1, ".") => words.ident(a + 2),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(SourceColumns::Named(names))
}

/// `t` or `s.t` at `start`; returns the token after it.
fn table_name(words: &Words, start: usize) -> Option<(Option<Ident>, Ident, usize)> {
    let first = words.ident(start)?;
    if words.is_char(start + 1, ".") {
        let second = words.ident(start + 2)?;
        if words.is_char(start + 3, ".") {
            return None;
        }
        return Some((Some(first), second, start + 3));
    }
    // A table function: `generate_series(1, 3)`.
    if words
        .tokens
        .get(start + 1)
        .is_some_and(|t| t.kind == TokenKind::OpenParen)
    {
        return None;
    }
    Some((None, first, start + 1))
}

/// `AS a`, a bare `a`, or nothing.
fn skip_alias(words: &Words, at: usize) -> usize {
    if words.is(at, "AS") {
        return if words.ident(at + 1).is_some() {
            at + 2
        } else {
            at
        };
    }
    let is_alias = words.ident(at).is_some() && !TRAILING.iter().any(|w| words.is(at, w));
    if is_alias { at + 1 } else { at }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
