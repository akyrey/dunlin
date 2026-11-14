//! A tolerant SQL tokenizer. It never fails: unterminated strings and comments run to the end.

use super::SplitterRules;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Word,
    QuotedIdent,
    String,
    Comment,
    Whitespace,
    Semicolon,
    OpenParen,
    CloseParen,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

impl Token {
    pub fn is_trivia(&self) -> bool {
        matches!(self.kind, TokenKind::Whitespace | TokenKind::Comment)
    }

    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

pub(crate) fn tokenize(source: &str, rules: SplitterRules) -> Vec<Token> {
    let mut lexer = Lexer {
        source,
        bytes: source.as_bytes(),
        pos: 0,
        rules,
    };
    let mut tokens = Vec::new();
    while lexer.pos < source.len() {
        let start = lexer.pos;
        let kind = lexer.next_kind();
        tokens.push(Token {
            kind,
            start,
            end: lexer.pos,
        });
    }
    tokens
}

struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    pos: usize,
    rules: SplitterRules,
}

impl Lexer<'_> {
    fn peek(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn current_char(&self) -> char {
        self.source[self.pos..].chars().next().unwrap_or('\0')
    }

    fn next_kind(&mut self) -> TokenKind {
        let c = self.current_char();
        match c {
            ';' => self.single(TokenKind::Semicolon),
            '(' => self.single(TokenKind::OpenParen),
            ')' => self.single(TokenKind::CloseParen),
            '\'' => self.quoted(b'\'', self.rules.backslash_escapes, TokenKind::String),
            '"' => self.quoted(b'"', self.rules.backslash_escapes, TokenKind::QuotedIdent),
            '`' if self.rules.backtick_identifiers => {
                self.quoted(b'`', false, TokenKind::QuotedIdent)
            }
            '[' if self.rules.bracket_identifiers => self.until_byte(b']'),
            '-' if self.peek(1) == Some(b'-') => self.line_comment(),
            '#' if self.rules.hash_comments => self.line_comment(),
            '/' if self.peek(1) == Some(b'*') => self.block_comment(),
            '$' if self.rules.dollar_quoting => self.dollar_quoted(),
            c if c.is_whitespace() => self.whitespace(),
            c if is_word_start(c) => self.word(),
            _ => self.single(TokenKind::Other),
        }
    }

    fn single(&mut self, kind: TokenKind) -> TokenKind {
        self.pos += self.current_char().len_utf8();
        kind
    }

    fn whitespace(&mut self) -> TokenKind {
        while self.pos < self.bytes.len() && self.current_char().is_whitespace() {
            self.pos += self.current_char().len_utf8();
        }
        TokenKind::Whitespace
    }

    fn word(&mut self) -> TokenKind {
        let start = self.pos;
        while self.pos < self.bytes.len() && is_word_continue(self.current_char()) {
            self.pos += self.current_char().len_utf8();
        }
        // PostgreSQL escape string: E'...' allows backslash escapes.
        let is_escape_prefix = self.pos - start == 1 && matches!(self.bytes[start], b'e' | b'E');
        if is_escape_prefix && self.peek(0) == Some(b'\'') {
            return self.quoted(b'\'', true, TokenKind::String);
        }
        TokenKind::Word
    }

    /// A quoted run where a doubled quote is an escape, and optionally `\x` too.
    fn quoted(&mut self, quote: u8, backslash: bool, kind: TokenKind) -> TokenKind {
        self.pos += 1;
        while let Some(b) = self.peek(0) {
            if backslash && b == b'\\' {
                self.pos = (self.pos + 2).min(self.bytes.len());
                self.align_to_char();
            } else if b == quote && self.peek(1) == Some(quote) {
                self.pos += 2;
            } else if b == quote {
                self.pos += 1;
                return kind;
            } else {
                self.pos += 1;
            }
        }
        kind
    }

    fn until_byte(&mut self, close: u8) -> TokenKind {
        self.pos += 1;
        match self.bytes[self.pos..].iter().position(|&b| b == close) {
            Some(offset) => self.pos += offset + 1,
            None => self.pos = self.bytes.len(),
        }
        TokenKind::QuotedIdent
    }

    fn line_comment(&mut self) -> TokenKind {
        match self.bytes[self.pos..].iter().position(|&b| b == b'\n') {
            Some(offset) => self.pos += offset,
            None => self.pos = self.bytes.len(),
        }
        TokenKind::Comment
    }

    fn block_comment(&mut self) -> TokenKind {
        self.pos += 2;
        let mut depth = 1usize;
        while self.pos < self.bytes.len() {
            if self.peek(0) == Some(b'*') && self.peek(1) == Some(b'/') {
                self.pos += 2;
                depth -= 1;
                if depth == 0 {
                    return TokenKind::Comment;
                }
            } else if self.rules.nested_block_comments
                && self.peek(0) == Some(b'/')
                && self.peek(1) == Some(b'*')
            {
                self.pos += 2;
                depth += 1;
            } else {
                self.pos += 1;
            }
        }
        TokenKind::Comment
    }

    /// `$tag$ ... $tag$` with an optional tag. A `$` that does not open one (`$1`) is `Other`.
    fn dollar_quoted(&mut self) -> TokenKind {
        let rest = &self.source[self.pos + 1..];
        let tag_len = rest
            .char_indices()
            .find(|&(i, c)| !(c == '_' || c.is_alphabetic() || (i > 0 && c.is_alphanumeric())))
            .map_or(rest.len(), |(i, _)| i);
        if !rest[tag_len..].starts_with('$') {
            return self.single(TokenKind::Other);
        }
        let delimiter = &self.source[self.pos..self.pos + tag_len + 2];
        let body_start = self.pos + delimiter.len();
        self.pos = match self.source[body_start..].find(delimiter) {
            Some(offset) => body_start + offset + delimiter.len(),
            None => self.bytes.len(),
        };
        TokenKind::String
    }

    fn align_to_char(&mut self) {
        while !self.source.is_char_boundary(self.pos) {
            self.pos += 1;
        }
    }
}

fn is_word_start(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

fn is_word_continue(c: char) -> bool {
    c == '_' || c == '$' || c.is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str, rules: SplitterRules) -> Vec<(TokenKind, &str)> {
        tokenize(source, rules)
            .iter()
            .map(|t| (t.kind, t.text(source)))
            .collect()
    }

    #[test]
    fn tokens_cover_the_input_contiguously() {
        let source = "SELECT 'a;b', \"c\" -- x\n/* y */ FROM t;";

        let tokens = tokenize(source, SplitterRules::default());

        assert_eq!(tokens.first().map(|t| t.start), Some(0));
        assert_eq!(tokens.last().map(|t| t.end), Some(source.len()));
        assert!(tokens.windows(2).all(|w| w[0].end == w[1].start));
    }

    #[test]
    fn escape_string_prefix_enables_backslashes() {
        let source = r"E'it\'s';";

        let result = kinds(source, SplitterRules::default());

        assert_eq!(
            result,
            vec![
                (TokenKind::String, r"E'it\'s'"),
                (TokenKind::Semicolon, ";")
            ]
        );
    }

    #[test]
    fn dollar_sign_without_a_tag_is_a_plain_character() {
        let rules = SplitterRules {
            dollar_quoting: true,
            ..SplitterRules::default()
        };

        let result = kinds("$1", rules);

        assert_eq!(
            result,
            vec![(TokenKind::Other, "$"), (TokenKind::Word, "1")]
        );
    }

    #[test]
    fn unterminated_string_runs_to_the_end() {
        let result = kinds("'abc; def", SplitterRules::default());

        assert_eq!(result, vec![(TokenKind::String, "'abc; def")]);
    }

    #[test]
    fn backslash_at_end_of_multibyte_text_does_not_split_a_char() {
        let rules = SplitterRules {
            backslash_escapes: true,
            ..SplitterRules::default()
        };

        let result = kinds("'\\é", rules);

        assert_eq!(result, vec![(TokenKind::String, "'\\é")]);
    }
}
