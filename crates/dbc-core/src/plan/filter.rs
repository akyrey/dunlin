//! Reads a plan's filter text for the one case an index suggestion covers (D-043): a
//! conjunction of simple comparisons of one column with constants. Anything else is `None`.

use crate::sql::SplitterRules;
use crate::sql::definition::unquote;
use crate::sql::lexer::{Token, TokenKind, tokenize};

/// Casts PostgreSQL adds to a `varchar`/`char` column by itself; an index on the column serves
/// them. Any other cast on the column is a different expression.
const IMPLICIT_CASTS: &[&str] = &["text", "bpchar"];

/// Words a constant may contain.
const CONSTANT_WORDS: &[&str] = &["NULL", "TRUE", "FALSE", "ANY", "ARRAY"];

#[derive(Debug, Clone, PartialEq)]
enum Term {
    /// A comparison of this column with constants.
    Column(String),
    /// `col IS NOT NULL`: optimisers add these for join columns; they do not count.
    Ignored,
}

/// The column every condition of `filter` compares with constants, when it is one column
/// qualified (if at all) by one of `qualifiers`.
pub fn single_column(filter: &str, qualifiers: &[&str], rules: SplitterRules) -> Option<String> {
    let tokens: Vec<Token> = tokenize(filter, rules)
        .into_iter()
        .filter(|t| !t.is_trivia())
        .collect();
    let reader = Reader {
        text: filter,
        qualifiers,
    };
    let mut column: Option<String> = None;
    for conjunct in reader.conjuncts(&tokens)? {
        match reader.term(conjunct)? {
            Term::Ignored => {}
            Term::Column(name) => match &column {
                Some(seen) if *seen != name => return None,
                _ => column = Some(name),
            },
        }
    }
    column
}

struct Reader<'a> {
    text: &'a str,
    qualifiers: &'a [&'a str],
}

impl Reader<'_> {
    fn word(&self, token: &Token) -> Option<String> {
        (token.kind == TokenKind::Word).then(|| token.text(self.text).to_ascii_uppercase())
    }

    fn is(&self, token: Option<&Token>, text: &str) -> bool {
        token.is_some_and(|t| t.text(self.text).eq_ignore_ascii_case(text))
    }

    /// Without parentheses that wrap the whole slice; `None` when they do not balance.
    fn unwrap<'t>(&self, mut tokens: &'t [Token]) -> Option<&'t [Token]> {
        while let (Some(first), Some(_)) = (tokens.first(), tokens.last()) {
            if first.kind != TokenKind::OpenParen || closing(tokens, 0)? != tokens.len() - 1 {
                break;
            }
            tokens = &tokens[1..tokens.len() - 1];
        }
        Some(tokens)
    }

    /// The `AND`ed parts at the top level; `None` for an `OR` or unbalanced text.
    fn conjuncts<'t>(&self, tokens: &'t [Token]) -> Option<Vec<&'t [Token]>> {
        let tokens = self.unwrap(tokens)?;
        if tokens.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        let (mut depth, mut start, mut between) = (0usize, 0usize, false);
        for (i, token) in tokens.iter().enumerate() {
            match token.kind {
                TokenKind::OpenParen => depth += 1,
                TokenKind::CloseParen => depth = depth.checked_sub(1)?,
                _ if depth > 0 => {}
                _ => match self.word(token).as_deref() {
                    Some("OR") => return None,
                    Some("BETWEEN") => between = true,
                    Some("AND") if between => between = false,
                    Some("AND") => {
                        parts.push(&tokens[start..i]);
                        start = i + 1;
                    }
                    _ => {}
                },
            }
        }
        if depth != 0 {
            return None;
        }
        parts.push(&tokens[start..]);
        Some(parts)
    }

    fn term(&self, tokens: &[Token]) -> Option<Term> {
        let tokens = self.unwrap(tokens)?;
        let (name, rest) = self.column(tokens)?;
        let words: Vec<Option<String>> = rest.iter().map(|t| self.word(t)).collect();
        let first = words.first().cloned().flatten();
        match first.as_deref() {
            Some("IS") => {
                let not = words.get(1).cloned().flatten().as_deref() == Some("NOT");
                let tail = if not { 2 } else { 1 };
                let null = rest.len() == tail + 1 && words[tail].as_deref() == Some("NULL");
                return match (null, not) {
                    (true, true) => Some(Term::Ignored),
                    (true, false) => Some(Term::Column(name)),
                    _ => None,
                };
            }
            Some("IN" | "BETWEEN") => {
                return self.constant(&rest[1..]).then_some(Term::Column(name));
            }
            _ => {}
        }
        let operator_end = rest
            .iter()
            .position(|t| t.kind != TokenKind::Other || !"<>=!".contains(t.text(self.text)))
            .unwrap_or(rest.len());
        let operator: String = rest[..operator_end]
            .iter()
            .map(|t| t.text(self.text))
            .collect();
        let comparison = matches!(operator.as_str(), "=" | "<" | "<=" | ">" | ">=");
        (comparison && self.constant(&rest[operator_end..])).then_some(Term::Column(name))
    }

    /// A column reference at the start: `col`, `q.col`, `(col)`, `(col)::text`. Returns its
    /// unquoted name and the tokens after it.
    fn column<'t>(&self, tokens: &'t [Token]) -> Option<(String, &'t [Token])> {
        let wrapped = tokens.first()?.kind == TokenKind::OpenParen;
        let mut at = usize::from(wrapped);
        let name_token = |i: usize| {
            tokens
                .get(i)
                .filter(|t| matches!(t.kind, TokenKind::Word | TokenKind::QuotedIdent))
        };
        let name_of = |token: &Token| match token.kind {
            TokenKind::QuotedIdent => unquote(token.text(self.text)),
            _ => token.text(self.text).to_owned(),
        };
        let mut name = name_of(name_token(at)?);
        if self.is(tokens.get(at + 1), ".") {
            let qualifier = name;
            if !self
                .qualifiers
                .iter()
                .any(|q| q.eq_ignore_ascii_case(&qualifier))
            {
                return None;
            }
            name = name_of(name_token(at + 2)?);
            at += 2;
        }
        at += 1;
        if CONSTANT_WORDS.contains(&name.to_ascii_uppercase().as_str()) {
            return None;
        }
        if wrapped {
            if tokens.get(at)?.kind != TokenKind::CloseParen {
                return None;
            }
            at += 1;
        }
        if self.is(tokens.get(at), ":") && self.is(tokens.get(at + 1), ":") {
            let cast = tokens.get(at + 2)?.text(self.text).to_ascii_lowercase();
            if !IMPLICIT_CASTS.contains(&cast.as_str()) {
                return None;
            }
            at += 3;
        }
        Some((name, &tokens[at..]))
    }

    /// Literals, parameters, casts and lists of them: no column, function or subquery.
    fn constant(&self, tokens: &[Token]) -> bool {
        if tokens.is_empty() {
            return false;
        }
        tokens
            .iter()
            .enumerate()
            .all(|(i, token)| match token.kind {
                TokenKind::String | TokenKind::OpenParen | TokenKind::CloseParen => true,
                TokenKind::Other => matches!(
                    token.text(self.text),
                    "," | "-" | "+" | ":" | "." | "$" | "[" | "]"
                ),
                TokenKind::Word => {
                    let text = token.text(self.text);
                    let upper = text.to_ascii_uppercase();
                    let after_cast = i >= 2
                        && self.is(tokens.get(i - 1), ":")
                        && self.is(tokens.get(i - 2), ":");
                    let parameter = i >= 1 && self.is(tokens.get(i - 1), "$");
                    // `AND` here can only be a `BETWEEN`'s: top-level ones were split off, and a
                    // subquery is refused by its other words.
                    text.starts_with(|c: char| c.is_ascii_digit())
                        || parameter
                        || after_cast
                        || upper == "AND"
                        || CONSTANT_WORDS.contains(&upper.as_str())
                }
                _ => false,
            })
    }
}

/// The index of the parenthesis that closes the one at `open`.
fn closing(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, token) in tokens.iter().enumerate().skip(open) {
        match token.kind {
            TokenKind::OpenParen => depth += 1,
            TokenKind::CloseParen => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
