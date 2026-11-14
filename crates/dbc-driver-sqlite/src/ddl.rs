//! Best-effort reading of the `CREATE TABLE` / `CREATE INDEX` text SQLite stores, for what no
//! pragma reports: check constraints, generated column expressions, index expressions and
//! partial index predicates.

use dbc_core::schema::Check;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    Quoted,
    Str,
    Open,
    Close,
    Comma,
    Other,
}

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

fn tokenize(sql: &str) -> Vec<Token> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let kind = match bytes[i] {
            b if b.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i = find_from(bytes, i, b"\n").map_or(bytes.len(), |p| p + 1);
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = find_from(bytes, i + 2, b"*/").map_or(bytes.len(), |p| p + 2);
                continue;
            }
            b'\'' => {
                i = skip_quoted(bytes, i, b'\'');
                Kind::Str
            }
            b'"' | b'`' => {
                i = skip_quoted(bytes, i, bytes[i]);
                Kind::Quoted
            }
            b'[' => {
                i = find_from(bytes, i, b"]").map_or(bytes.len(), |p| p + 1);
                Kind::Quoted
            }
            b'(' => {
                i += 1;
                Kind::Open
            }
            b')' => {
                i += 1;
                Kind::Close
            }
            b',' => {
                i += 1;
                Kind::Comma
            }
            b if is_word(b) => {
                while i < bytes.len() && is_word(bytes[i]) {
                    i += 1;
                }
                Kind::Word
            }
            _ => {
                i += 1;
                Kind::Other
            }
        };
        tokens.push(Token {
            kind,
            start,
            end: i,
        });
    }
    tokens
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

fn find_from(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Index just past the closing quote; a doubled quote is an escaped one.
fn skip_quoted(bytes: &[u8], open: usize, quote: u8) -> usize {
    let mut i = open + 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            if bytes.get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

/// The identifier a token names, with quotes removed.
fn identifier(sql: &str, token: Token) -> String {
    let text = &sql[token.start..token.end];
    match token.kind {
        Kind::Quoted if text.len() >= 2 => {
            let (open, inner) = (&text[..1], &text[1..text.len() - 1]);
            match open {
                "[" => inner.to_owned(),
                quote => inner.replace(&format!("{quote}{quote}"), quote),
            }
        }
        Kind::Str if text.len() >= 2 => text[1..text.len() - 1].replace("''", "'"),
        _ => text.to_owned(),
    }
}

fn is_keyword(sql: &str, token: Token, keyword: &str) -> bool {
    token.kind == Kind::Word && sql[token.start..token.end].eq_ignore_ascii_case(keyword)
}

/// Index of the `Close` matching the `Open` at `open`.
fn matching_close(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, token) in tokens.iter().enumerate().skip(open) {
        match token.kind {
            Kind::Open => depth += 1,
            Kind::Close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Splits `tokens[from..to]` at commas outside parentheses.
fn split_items(tokens: &[Token], from: usize, to: usize) -> Vec<(usize, usize)> {
    let mut items = Vec::new();
    let mut depth = 0usize;
    let mut start = from;
    for (i, token) in tokens.iter().enumerate().take(to).skip(from) {
        match token.kind {
            Kind::Open => depth += 1,
            Kind::Close => depth = depth.saturating_sub(1),
            Kind::Comma if depth == 0 => {
                items.push((start, i));
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < to {
        items.push((start, to));
    }
    items
}

/// Source text between two tokens (exclusive of the parentheses themselves).
fn inner_text(sql: &str, tokens: &[Token], open: usize, close: usize) -> String {
    sql[tokens[open].end..tokens[close].start].trim().to_owned()
}

/// What the `CREATE TABLE` text says beyond the pragmas.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct TableDdl {
    /// (column name, generation expression)
    pub generated: Vec<(String, String)>,
    pub checks: Vec<Check>,
}

pub(crate) fn parse_table(sql: &str) -> TableDdl {
    let tokens = tokenize(sql);
    let mut ddl = TableDdl::default();
    let Some(open) = tokens.iter().position(|t| t.kind == Kind::Open) else {
        return ddl;
    };
    let Some(close) = matching_close(&tokens, open) else {
        return ddl;
    };
    for (from, to) in split_items(&tokens, open + 1, close) {
        parse_item(sql, &tokens[from..to], &mut ddl);
    }
    ddl
}

const TABLE_CONSTRAINTS: [&str; 5] = ["CONSTRAINT", "PRIMARY", "UNIQUE", "CHECK", "FOREIGN"];

fn parse_item(sql: &str, item: &[Token], ddl: &mut TableDdl) {
    let Some(&first) = item.first() else {
        return;
    };
    let is_column = !TABLE_CONSTRAINTS.iter().any(|k| is_keyword(sql, first, k));
    let column = is_column.then(|| identifier(sql, first));
    let mut pending_name: Option<String> = None;
    let mut i = usize::from(is_column);
    while i < item.len() {
        let token = item[i];
        let next_is_group = item.get(i + 1).is_some_and(|t| t.kind == Kind::Open);
        if is_keyword(sql, token, "CONSTRAINT") {
            pending_name = item.get(i + 1).map(|t| identifier(sql, *t));
            i += 2;
            continue;
        }
        if token.kind == Kind::Open {
            i = matching_close(item, i).map_or(item.len(), |c| c + 1);
            continue;
        }
        let Some(close) = next_is_group.then(|| matching_close(item, i + 1)).flatten() else {
            i += 1;
            continue;
        };
        if is_keyword(sql, token, "CHECK") {
            ddl.checks.push(Check {
                name: pending_name.take(),
                expression: inner_text(sql, item, i + 1, close),
            });
        } else if is_keyword(sql, token, "AS") {
            if let Some(column) = &column {
                let expression = inner_text(sql, item, i + 1, close);
                ddl.generated.push((column.clone(), expression));
            }
        }
        i = close + 1;
    }
}

/// What the `CREATE INDEX` text says beyond the pragmas.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct IndexDdl {
    /// Each key part's source text, in order.
    pub parts: Vec<String>,
    pub predicate: Option<String>,
}

pub(crate) fn parse_index(sql: &str) -> IndexDdl {
    let tokens = tokenize(sql);
    let mut ddl = IndexDdl::default();
    let Some(on) = tokens.iter().position(|t| is_keyword(sql, *t, "ON")) else {
        return ddl;
    };
    let Some(open) = tokens
        .iter()
        .skip(on)
        .position(|t| t.kind == Kind::Open)
        .map(|p| p + on)
    else {
        return ddl;
    };
    let Some(close) = matching_close(&tokens, open) else {
        return ddl;
    };
    ddl.parts = split_items(&tokens, open + 1, close)
        .into_iter()
        .map(|(from, to)| sql[tokens[from].start..tokens[to - 1].end].to_owned())
        .collect();
    if let Some(&where_) = tokens
        .get(close + 1)
        .filter(|t| is_keyword(sql, **t, "WHERE"))
    {
        let predicate = sql[where_.end..].trim().trim_end_matches(';').trim();
        ddl.predicate = (!predicate.is_empty()).then(|| predicate.to_owned());
    }
    ddl
}

#[cfg(test)]
mod fuzz {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        /// DDL is read from `sqlite_master`, which any program can write (task 8.16).
        #[test]
        fn no_ddl_text_panics_the_parser(
            sql in prop_oneof!["\\PC{0,80}", "[(),'\"`\\[\\] a-zA-Z;\n]{0,80}"],
        ) {
            let _ = parse_table(&sql);
            let _ = parse_index(&sql);
            let _ = parse_table(&format!("CREATE TABLE t ({sql}"));
            let _ = parse_index(&format!("CREATE INDEX i ON t ({sql}) WHERE {sql}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(name: Option<&str>, expression: &str) -> Check {
        Check {
            name: name.map(str::to_owned),
            expression: expression.to_owned(),
        }
    }

    #[test]
    fn reads_column_and_table_checks_with_their_names() {
        let ddl = parse_table(
            "CREATE TABLE t (a INT CHECK (a > 0), b NUMERIC(10, 2) CONSTRAINT b_ok CHECK (b < (1 + 2)), \
             CONSTRAINT pair CHECK (a <> b), CHECK (length('x,)') = 3))",
        );

        assert_eq!(
            ddl.checks,
            vec![
                check(None, "a > 0"),
                check(Some("b_ok"), "b < (1 + 2)"),
                check(Some("pair"), "a <> b"),
                check(None, "length('x,)') = 3"),
            ]
        );
    }

    #[test]
    fn reads_generated_expressions_by_column() {
        let ddl = parse_table(
            "CREATE TABLE \"we\"\"ird\" (\"a b\" TEXT, full TEXT GENERATED ALWAYS AS (a || ' ' || b) STORED, \
             [n] INT AS (1) VIRTUAL, d TEXT DEFAULT (lower('AS (x)')))",
        );

        assert_eq!(
            ddl.generated,
            vec![
                ("full".to_owned(), "a || ' ' || b".to_owned()),
                ("n".to_owned(), "1".to_owned()),
            ]
        );
        assert!(ddl.checks.is_empty());
    }

    #[test]
    fn ignores_comments_and_unterminated_text() {
        let ddl =
            parse_table("CREATE TABLE t (a INT -- CHECK (no)\n, /* CHECK (no) */ b INT CHECK (b)");

        assert_eq!(ddl, TableDdl::default());
        assert_eq!(
            parse_table("CREATE TABLE t AS SELECT 1"),
            TableDdl::default()
        );
    }

    #[test]
    fn reads_index_parts_and_predicate() {
        let ddl = parse_index(
            "CREATE INDEX \"i\" ON \"t\" (lower(last_name), \"a b\" DESC) WHERE salary > 100000",
        );

        assert_eq!(ddl.parts, vec!["lower(last_name)", "\"a b\" DESC"]);
        assert_eq!(ddl.predicate.as_deref(), Some("salary > 100000"));
        assert_eq!(parse_index("CREATE INDEX i ON t (a)").predicate, None);
        assert_eq!(parse_index("nonsense"), IndexDdl::default());
    }

    #[test]
    fn unquotes_every_identifier_style() {
        let sql = "\"a\"\"b\" `c``d` [e f] 'g''h' plain";
        let names: Vec<String> = tokenize(sql)
            .into_iter()
            .map(|t| identifier(sql, t))
            .collect();

        assert_eq!(names, vec!["a\"b", "c`d", "e f", "g'h", "plain"]);
    }
}
