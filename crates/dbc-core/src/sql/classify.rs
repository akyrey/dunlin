//! Statement classification for safeguards. A lexer-level heuristic, not access control.

use super::SplitterRules;
use super::lexer::{TokenKind, tokenize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum StatementClass {
    Read,
    Write,
    Ddl,
    Transaction,
    Session,
    Unknown,
}

impl StatementClass {
    /// Read-only sessions reject everything else, including `Unknown`.
    pub fn allowed_on_read_only(self) -> bool {
        matches!(self, Self::Read | Self::Session | Self::Transaction)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Danger {
    UpdateWithoutWhere,
    DeleteWithoutWhere,
    Drop,
    Truncate,
}

impl Danger {
    /// Shown to the user when asking for confirmation.
    pub fn reason(self) -> &'static str {
        match self {
            Self::UpdateWithoutWhere => "UPDATE without WHERE affects every row",
            Self::DeleteWithoutWhere => "DELETE without WHERE affects every row",
            Self::Drop => "DROP permanently removes the object",
            Self::Truncate => "TRUNCATE removes every row",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classification {
    pub class: StatementClass,
    pub danger: Option<Danger>,
}

/// Classifies one statement. Leading comments and whitespace are ignored.
pub fn classify(statement: &str, rules: SplitterRules) -> Classification {
    let tokens = significant_tokens(statement, rules);
    classify_tokens(&tokens)
}

/// A non-trivia token with its parenthesis depth. `(` carries the outer depth, `)` too.
#[derive(Debug, Clone, Copy)]
struct Sig<'a> {
    kind: TokenKind,
    text: &'a str,
    depth: usize,
}

impl Sig<'_> {
    fn is(&self, keyword: &str) -> bool {
        self.kind == TokenKind::Word && self.text.eq_ignore_ascii_case(keyword)
    }

    fn is_any(&self, keywords: &[&str]) -> bool {
        keywords.iter().any(|k| self.is(k))
    }
}

fn significant_tokens(statement: &str, rules: SplitterRules) -> Vec<Sig<'_>> {
    let mut depth = 0usize;
    let mut result = Vec::new();
    for token in tokenize(statement, rules).iter().filter(|t| !t.is_trivia()) {
        let token_depth = match token.kind {
            TokenKind::OpenParen => {
                depth += 1;
                depth - 1
            }
            TokenKind::CloseParen => {
                depth = depth.saturating_sub(1);
                depth
            }
            _ => depth,
        };
        result.push(Sig {
            kind: token.kind,
            text: token.text(statement),
            depth: token_depth,
        });
    }
    result
}

const READ_HEADS: &[&str] = &["VALUES", "TABLE", "SHOW", "DESCRIBE", "DESC"];
const WRITE_HEADS: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "MERGE", "REPLACE", "UPSERT", "COPY", "CALL", "EXEC", "EXECUTE",
    "DO", "LOAD", "VACUUM", "ANALYZE",
];
const DDL_HEADS: &[&str] = &[
    "CREATE", "ALTER", "DROP", "TRUNCATE", "RENAME", "COMMENT", "GRANT", "REVOKE", "REINDEX",
];
const TRANSACTION_HEADS: &[&str] = &[
    "BEGIN",
    "START",
    "COMMIT",
    "ROLLBACK",
    "SAVEPOINT",
    "RELEASE",
    "END",
    "ABORT",
];
const SESSION_HEADS: &[&str] = &[
    "SET", "RESET", "USE", "ATTACH", "DETACH", "DISCARD", "LISTEN", "UNLISTEN",
];
/// Statement keywords that `EXPLAIN` may precede.
const EXPLAINABLE: &[&str] = &[
    "SELECT", "WITH", "INSERT", "UPDATE", "DELETE", "MERGE", "VALUES", "TABLE", "CREATE",
];
const MODIFYING: &[&str] = &["INSERT", "UPDATE", "DELETE", "MERGE"];

fn classify_tokens(tokens: &[Sig<'_>]) -> Classification {
    let Some(head_index) = tokens.iter().position(|t| t.kind != TokenKind::OpenParen) else {
        return Classification {
            class: StatementClass::Unknown,
            danger: None,
        };
    };
    let head = tokens[head_index];
    let class = if head.is("SELECT") {
        select_class(&tokens[head_index..], head.depth)
    } else if head.is("WITH") {
        with_class(tokens)
    } else if head.is("EXPLAIN") {
        return explain(&tokens[head_index + 1..]);
    } else if head.is("PRAGMA") {
        pragma_class(tokens)
    } else if head.is_any(READ_HEADS) {
        StatementClass::Read
    } else if head.is_any(WRITE_HEADS) {
        StatementClass::Write
    } else if head.is_any(DDL_HEADS) {
        StatementClass::Ddl
    } else if head.is_any(TRANSACTION_HEADS) {
        escalate_if_read_write(tokens, StatementClass::Transaction)
    } else if head.is_any(SESSION_HEADS) {
        escalate_if_read_write(tokens, StatementClass::Session)
    } else {
        StatementClass::Unknown
    };
    let danger = danger(tokens, head, class);
    Classification { class, danger }
}

/// `SELECT ... INTO` creates a table (PostgreSQL) or writes a file (MySQL).
fn select_class(tokens: &[Sig<'_>], depth: usize) -> StatementClass {
    if tokens.iter().any(|t| t.depth == depth && t.is("INTO")) {
        StatementClass::Write
    } else {
        StatementClass::Read
    }
}

fn with_class(tokens: &[Sig<'_>]) -> StatementClass {
    if modifying_heads(tokens).next().is_some() {
        StatementClass::Write
    } else {
        StatementClass::Read
    }
}

fn pragma_class(tokens: &[Sig<'_>]) -> StatementClass {
    if tokens.iter().any(|t| t.depth == 0 && t.text == "=") {
        StatementClass::Session
    } else {
        StatementClass::Read
    }
}

/// `EXPLAIN ANALYZE` runs the statement, so it is classified as the statement itself.
fn explain(rest: &[Sig<'_>]) -> Classification {
    let read = Classification {
        class: StatementClass::Read,
        danger: None,
    };
    let Some(inner) = rest
        .iter()
        .position(|t| t.depth == 0 && t.is_any(EXPLAINABLE))
    else {
        return read;
    };
    if rest[..inner].iter().any(|t| t.is("ANALYZE")) {
        classify_tokens(&rest[inner..])
    } else {
        read
    }
}

/// Statements that would undo the server-side read-only guard count as writes.
fn escalate_if_read_write(tokens: &[Sig<'_>], class: StatementClass) -> StatementClass {
    let mentions_read_only = tokens
        .iter()
        .any(|t| t.kind == TokenKind::Word && t.text.to_ascii_lowercase().contains("read_only"));
    let read_write = tokens
        .windows(2)
        .any(|w| w[0].is("READ") && w[1].is("WRITE"));
    let reset_all = tokens.len() >= 2 && tokens[0].is("RESET") && tokens[1].is("ALL");
    if mentions_read_only || read_write || reset_all {
        StatementClass::Write
    } else {
        class
    }
}

fn danger(tokens: &[Sig<'_>], head: Sig<'_>, class: StatementClass) -> Option<Danger> {
    if head.is("DROP") {
        return Some(Danger::Drop);
    }
    if head.is("TRUNCATE") {
        return Some(Danger::Truncate);
    }
    if class != StatementClass::Write {
        return None;
    }
    modifying_heads(tokens).find_map(|index| {
        let token = tokens[index];
        let unfiltered = !has_where(&tokens[index + 1..], token.depth);
        if token.is("UPDATE") && unfiltered {
            Some(Danger::UpdateWithoutWhere)
        } else if token.is("DELETE") && unfiltered {
            Some(Danger::DeleteWithoutWhere)
        } else {
            None
        }
    })
}

/// Indexes of data-modifying keywords that start a (sub)statement: at the start, or right after
/// a parenthesis (a CTE body, or the main statement after a CTE list). This excludes
/// `FOR UPDATE`, `DO UPDATE` and `ON DUPLICATE KEY UPDATE`.
fn modifying_heads<'a>(tokens: &'a [Sig<'a>]) -> impl Iterator<Item = usize> + 'a {
    tokens.iter().enumerate().filter_map(|(index, token)| {
        let starts_statement = index == 0
            || matches!(
                tokens[index - 1].kind,
                TokenKind::OpenParen | TokenKind::CloseParen
            );
        (starts_statement && token.is_any(MODIFYING)).then_some(index)
    })
}

/// Whether a `WHERE` follows at `depth` before the enclosing parenthesis closes.
fn has_where(rest: &[Sig<'_>], depth: usize) -> bool {
    rest.iter()
        .take_while(|t| t.depth >= depth)
        .any(|t| t.depth == depth && t.is("WHERE"))
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
