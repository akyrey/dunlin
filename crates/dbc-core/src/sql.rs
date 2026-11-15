//! Lexer-level SQL utilities: statement splitting and classification.

mod classify;
mod lexer;
mod source;
mod split;

pub use classify::{Classification, Danger, StatementClass, classify};
pub use source::{Ident, SourceColumns, SourceTable, single_table_source};
pub use split::{StatementRange, split};

/// Dialect-specific lexical rules. The default is ANSI: `'strings'`, `"identifiers"`,
/// `--` and non-nesting `/* */` comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SplitterRules {
    /// PostgreSQL `$$ ... $$` and `$tag$ ... $tag$`.
    pub dollar_quoting: bool,
    /// PostgreSQL: `/* /* */ */` nests.
    pub nested_block_comments: bool,
    /// MySQL and SQLite `` `identifiers` ``.
    pub backtick_identifiers: bool,
    /// SQLite `[identifiers]`.
    pub bracket_identifiers: bool,
    /// MySQL `# comments`.
    pub hash_comments: bool,
    /// MySQL: backslash escapes inside quoted strings.
    pub backslash_escapes: bool,
}

/// Converts a byte offset in `text` to a UTF-16 code unit offset, as used by JavaScript.
/// Offsets past the end are clamped to the end.
pub fn byte_to_utf16(text: &str, byte_offset: usize) -> usize {
    text.char_indices()
        .take_while(|(index, _)| *index < byte_offset)
        .map(|(_, c)| c.len_utf16())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_to_utf16_counts_surrogate_pairs() {
        let text = "é🌍x";

        assert_eq!(byte_to_utf16(text, 0), 0);
        assert_eq!(byte_to_utf16(text, 2), 1);
        assert_eq!(byte_to_utf16(text, 6), 3);
        assert_eq!(byte_to_utf16(text, 7), 4);
        assert_eq!(byte_to_utf16(text, 100), 4);
    }
}
