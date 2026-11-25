//! Splits `CREATE TABLE` text into pieces that keep their text as written (task 9.5a, D-042).
//!
//! Schema edits on MySQL and SQLite change a table by editing the server's own `CREATE TABLE`
//! text, so clauses the model does not know (collations, `AUTO_INCREMENT`, `ON UPDATE`, conflict
//! clauses, table options) survive. This is a tolerant splitter over the lexer, not a parser: it
//! finds top-level commas, and in a column the keywords that start a clause.

use super::SplitterRules;
use super::lexer::{Token, TokenKind, tokenize};

/// `head` is everything up to and including the opening parenthesis, `items` the column
/// definitions and table constraints, trimmed, and `tail` the closing parenthesis and the table
/// options after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTableParts {
    pub head: String,
    pub items: Vec<String>,
    pub tail: String,
}

impl CreateTableParts {
    pub fn to_sql(&self, separator: &str) -> String {
        format!("{}{}{}", self.head, self.items.join(separator), self.tail)
    }
}

/// One clause of a column definition: `NOT NULL`, `DEFAULT 'x'`, `COLLATE NOCASE`, ...
/// `keyword` is its first word in upper case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause {
    pub keyword: String,
    pub text: String,
}

/// A column definition: its name as written (quotes kept), its type (possibly empty on SQLite)
/// and its clauses in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnParts {
    pub name: String,
    pub type_text: String,
    pub clauses: Vec<Clause>,
}

impl ColumnParts {
    pub fn to_sql(&self) -> String {
        std::iter::once(self.name.as_str())
            .chain(Some(self.type_text.as_str()).filter(|t| !t.is_empty()))
            .chain(self.clauses.iter().map(|c| c.text.as_str()))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Words that start a table constraint rather than a column definition.
const TABLE_CONSTRAINTS: &[&str] = &[
    "CONSTRAINT",
    "PRIMARY",
    "UNIQUE",
    "KEY",
    "INDEX",
    "FOREIGN",
    "CHECK",
    "FULLTEXT",
    "SPATIAL",
];

/// Words that start a clause of a column definition (MySQL, MariaDB and SQLite).
const CLAUSE_WORDS: &[&str] = &[
    "CONSTRAINT",
    "NOT",
    "NULL",
    "DEFAULT",
    "COLLATE",
    "CHARACTER",
    "CHARSET",
    "COMMENT",
    "AUTO_INCREMENT",
    "ON",
    "PRIMARY",
    "UNIQUE",
    "KEY",
    "CHECK",
    "REFERENCES",
    "GENERATED",
    "AS",
    "VISIBLE",
    "INVISIBLE",
    "SRID",
    "STORAGE",
    "COLUMN_FORMAT",
    "ENGINE_ATTRIBUTE",
    "SECONDARY_ENGINE_ATTRIBUTE",
];

fn significant(text: &str, rules: SplitterRules) -> Vec<Token> {
    tokenize(text, rules)
        .into_iter()
        .filter(|t| !t.is_trivia())
        .collect()
}

fn is_comma(token: &Token, text: &str) -> bool {
    token.kind == TokenKind::Other && token.text(text) == ","
}

fn upper_word(token: &Token, text: &str) -> Option<String> {
    (token.kind == TokenKind::Word).then(|| token.text(text).to_ascii_uppercase())
}

/// The pieces of a `CREATE TABLE ... ( ... ) ...` statement, or `None` for anything else
/// (a view, `CREATE TABLE ... AS SELECT`, unbalanced text).
pub fn split_create_table(text: &str, rules: SplitterRules) -> Option<CreateTableParts> {
    let tokens = significant(text, rules);
    let words: Vec<Option<String>> = tokens.iter().map(|t| upper_word(t, text)).collect();
    if words.first()?.as_deref() != Some("CREATE") {
        return None;
    }
    let open = tokens.iter().position(|t| t.kind == TokenKind::OpenParen)?;
    let before = &words[..open];
    let is_table = before.iter().any(|w| w.as_deref() == Some("TABLE"));
    if !is_table || before.iter().any(|w| w.as_deref() == Some("AS")) {
        return None;
    }
    let mut depth = 0usize;
    let mut item_start = tokens[open].end;
    let mut items = Vec::new();
    for token in &tokens[open + 1..] {
        match token.kind {
            TokenKind::OpenParen => depth += 1,
            TokenKind::CloseParen if depth == 0 => {
                items.push(text[item_start..token.start].trim().to_owned());
                return Some(CreateTableParts {
                    head: text[..tokens[open].end].to_owned(),
                    items: items.into_iter().filter(|i| !i.is_empty()).collect(),
                    tail: text[token.start..].to_owned(),
                });
            }
            TokenKind::CloseParen => depth -= 1,
            _ if depth == 0 && is_comma(token, text) => {
                items.push(text[item_start..token.start].trim().to_owned());
                item_start = token.end;
            }
            _ => {}
        }
    }
    None
}

/// The name an item defines when it is a column, unquoted; `None` for a table constraint.
pub fn column_name(item: &str, rules: SplitterRules) -> Option<String> {
    let first = *significant(item, rules).first()?;
    let text = first.text(item);
    match first.kind {
        TokenKind::Word if TABLE_CONSTRAINTS.contains(&text.to_ascii_uppercase().as_str()) => None,
        TokenKind::Word => Some(text.to_owned()),
        TokenKind::QuotedIdent => Some(unquote(text)),
        _ => None,
    }
}

/// `"a ""b"""` → `a "b"`; `[a b]` → `a b`; `` `a``b` `` → ``a`b``.
pub fn unquote(text: &str) -> String {
    let mut chars = text.chars();
    let (Some(open), Some(_)) = (chars.next(), chars.next_back()) else {
        return text.to_owned();
    };
    let inner = chars.as_str();
    match open {
        '[' => inner.to_owned(),
        quote => inner.replace(&format!("{quote}{quote}"), &quote.to_string()),
    }
}

/// Splits one column definition. `None` when `item` holds more than one item.
pub fn split_column(item: &str, rules: SplitterRules) -> Option<ColumnParts> {
    let tokens = significant(item, rules);
    let (name, rest) = tokens.split_first()?;
    let starts = clause_starts(item, rest)?;
    let type_end = starts.first().map_or(item.len(), |&i| rest[i].start);
    let clauses = starts
        .iter()
        .enumerate()
        .map(|(n, &i)| {
            let end = starts.get(n + 1).map_or(item.len(), |&j| rest[j].start);
            let text = item[rest[i].start..end].trim().to_owned();
            Clause {
                keyword: rest[i].text(item).to_ascii_uppercase(),
                text,
            }
        })
        .collect();
    Some(ColumnParts {
        name: name.text(item).to_owned(),
        type_text: item[name.end..type_end].trim().to_owned(),
        clauses,
    })
}

/// What the current clause lets through without starting a new one.
#[derive(Default)]
struct ClauseState {
    /// The clause kind that decides continuations (after `CONSTRAINT name`, the constraint's).
    kind: Option<String>,
    /// `DEFAULT` and `ON UPDATE` take one value next, whatever word it is.
    value_pending: bool,
    /// After `CONSTRAINT name`, the next keyword belongs to it.
    merge_next: bool,
}

impl ClauseState {
    fn continues(&self, word: &str, next: Option<&str>) -> bool {
        if self.merge_next || (word == "ON" && next == Some("CONFLICT")) {
            return true;
        }
        let allowed: &[&str] = match self.kind.as_deref() {
            Some("REFERENCES") => &["ON", "MATCH", "DEFERRABLE", "INITIALLY", "NULL", "DEFAULT"],
            Some("GENERATED") => &["ALWAYS", "AS", "STORED", "VIRTUAL", "BY", "DEFAULT"],
            Some("AS") => &["STORED", "VIRTUAL"],
            Some("PRIMARY") => &["KEY", "ASC", "DESC", "AUTOINCREMENT"],
            Some("UNIQUE") => &["KEY"],
            Some("NOT") => &["NULL"],
            _ => &[],
        };
        allowed.contains(&word)
            || (self.kind.as_deref() == Some("REFERENCES")
                && word == "NOT"
                && next == Some("DEFERRABLE"))
    }
}

/// Indexes into `tokens` (the tokens after the name) where a clause starts, or `None` when the
/// text holds a top-level comma.
fn clause_starts(text: &str, tokens: &[Token]) -> Option<Vec<usize>> {
    let words: Vec<Option<String>> = tokens.iter().map(|t| upper_word(t, text)).collect();
    let mut starts = Vec::new();
    let mut state = ClauseState::default();
    let mut depth = 0usize;
    for (i, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::OpenParen => depth += 1,
            TokenKind::CloseParen => depth = depth.saturating_sub(1),
            _ => {}
        }
        let at_top = depth == 0 || (depth == 1 && token.kind == TokenKind::OpenParen);
        if !at_top {
            continue;
        }
        if depth == 0 && is_comma(token, text) {
            return None;
        }
        if state.value_pending {
            let sign = matches!(token.text(text), "-" | "+");
            state.value_pending = sign;
            continue;
        }
        let Some(word) = words[i].as_deref() else {
            continue;
        };
        if !CLAUSE_WORDS.contains(&word) && !(state.kind.is_some() && word == "UPDATE") {
            continue;
        }
        let next = words.get(i + 1).and_then(Option::as_deref);
        if word == "CHARACTER" && next != Some("SET") {
            continue;
        }
        if state.kind.is_some() && state.continues(word, next) {
            if state.merge_next {
                state.merge_next = false;
                state.kind = Some(word.to_owned());
            }
            continue;
        }
        if word == "UPDATE" {
            // `ON UPDATE <value>` (MySQL timestamps).
            state.value_pending = state.kind.as_deref() == Some("ON");
            continue;
        }
        starts.push(i);
        state = ClauseState {
            kind: Some(word.to_owned()),
            value_pending: word == "DEFAULT",
            merge_next: word == "CONSTRAINT",
        };
    }
    Some(starts)
}

#[cfg(test)]
#[path = "definition_tests.rs"]
mod tests;
