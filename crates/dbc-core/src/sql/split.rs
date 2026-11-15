//! Splits a script into statements on `;` outside strings, identifiers, comments and blocks.

use super::SplitterRules;
use super::lexer::{Token, TokenKind, tokenize};

/// Byte range of one statement, excluding surrounding whitespace and the terminating `;`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatementRange {
    pub start: usize,
    pub end: usize,
}

/// Splits `text` into statements. Segments that contain only comments are dropped.
///
/// Inside a `CREATE` statement, `BEGIN ... END` blocks (SQLite triggers, SQL-standard function
/// bodies) do not end on `;`. With `delimiter_command`, a `DELIMITER x` line at the start of a
/// statement is not a statement: it makes `x` end statements until the next `DELIMITER`.
pub fn split(text: &str, rules: SplitterRules) -> Vec<StatementRange> {
    let tokens = tokenize(text, rules);
    let mut ranges = Vec::new();
    let mut current = Segment::default();
    // `None`: statements end on `;`.
    let mut delimiter: Option<&str> = None;
    // Tokens before this byte offset were consumed by a delimiter or a `DELIMITER` line.
    let mut skip_to = 0;
    for (index, token) in tokens.iter().enumerate() {
        if token.start < skip_to {
            continue;
        }
        if rules.delimiter_command
            && !current.has_code
            && token.kind == TokenKind::Word
            && token.text(text).eq_ignore_ascii_case("DELIMITER")
        {
            let rest = text.get(token.end..).unwrap_or_default();
            let line_end = token.end + rest.find('\n').unwrap_or(rest.len());
            if let Some(chosen) = text
                .get(token.end..line_end)
                .and_then(|line| line.split_whitespace().next())
            {
                delimiter = (chosen != ";").then_some(chosen);
                current = Segment::default();
                skip_to = line_end;
                continue;
            }
        }
        if let Some(chosen) = delimiter {
            let can_end = !matches!(
                token.kind,
                TokenKind::String | TokenKind::QuotedIdent | TokenKind::Comment
            );
            if can_end
                && text
                    .get(token.start..)
                    .is_some_and(|t| t.starts_with(chosen))
            {
                ranges.extend(current.finish());
                current = Segment::default();
                skip_to = token.start + chosen.len();
                continue;
            }
            current.push_plain(text, token);
            continue;
        }
        if token.kind == TokenKind::Semicolon && current.block_depth == 0 {
            ranges.extend(current.finish());
            current = Segment::default();
            continue;
        }
        current.push(
            text,
            token,
            next_word(text, tokens.get(index + 1..).unwrap_or_default()),
        );
    }
    ranges.extend(current.finish());
    ranges
}

#[derive(Default)]
struct Segment {
    start: Option<usize>,
    end: usize,
    has_code: bool,
    is_create: bool,
    block_depth: usize,
}

impl Segment {
    fn push(&mut self, text: &str, token: &Token, next_word: Option<&str>) {
        if token.kind == TokenKind::Whitespace {
            return;
        }
        self.start.get_or_insert(token.start);
        // Line comments and unterminated quotes run on and may end in whitespace.
        self.end = token.start + token.text(text).trim_end().len();
        if token.kind == TokenKind::Comment {
            return;
        }
        if token.kind == TokenKind::Word {
            self.track_blocks(token.text(text), next_word);
        }
        self.has_code = true;
    }

    /// Under a custom delimiter only the delimiter ends a statement: no block tracking.
    fn push_plain(&mut self, text: &str, token: &Token) {
        if token.kind == TokenKind::Whitespace {
            return;
        }
        self.start.get_or_insert(token.start);
        self.end = token.start + token.text(text).trim_end().len();
        if token.kind != TokenKind::Comment {
            self.has_code = true;
        }
    }

    fn track_blocks(&mut self, word: &str, next_word: Option<&str>) {
        if !self.has_code {
            self.is_create = word.eq_ignore_ascii_case("CREATE");
            return;
        }
        if !self.is_create {
            return;
        }
        if word.eq_ignore_ascii_case("BEGIN")
            || (self.block_depth > 0 && word.eq_ignore_ascii_case("CASE"))
        {
            self.block_depth += 1;
        } else if self.block_depth > 0
            && word.eq_ignore_ascii_case("END")
            && !next_word.is_some_and(closes_compound_statement)
        {
            self.block_depth -= 1;
        }
    }

    fn finish(&self) -> Option<StatementRange> {
        match self.start {
            Some(start) if self.has_code => Some(StatementRange {
                start,
                end: self.end,
            }),
            _ => None,
        }
    }
}

/// `END IF`, `END LOOP`, ...: these close constructs that `BEGIN`/`CASE` did not open.
fn closes_compound_statement(word: &str) -> bool {
    ["IF", "LOOP", "WHILE", "REPEAT"]
        .iter()
        .any(|w| word.eq_ignore_ascii_case(w))
}

fn next_word<'a>(text: &'a str, rest: &[Token]) -> Option<&'a str> {
    rest.iter()
        .find(|t| !t.is_trivia())
        .filter(|t| t.kind == TokenKind::Word)
        .map(|t| t.text(text))
}

#[cfg(test)]
#[path = "split_tests.rs"]
mod tests;
