//! SQL dialect: quoting and lexical rules.

use crate::schema::TableRef;
use crate::sql::SplitterRules;

pub trait Dialect: Send + Sync {
    /// Quotes an identifier, doubling any embedded quote character.
    fn quote_identifier(&self, name: &str) -> String;

    /// `catalog.schema.name`, each part quoted, omitting absent parts.
    fn qualified_name(&self, table: &TableRef) -> String {
        let namespace = &table.namespace;
        [
            namespace.catalog.as_deref(),
            namespace.schema.as_deref(),
            Some(table.name.as_str()),
        ]
        .into_iter()
        .flatten()
        .map(|part| self.quote_identifier(part))
        .collect::<Vec<_>>()
        .join(".")
    }

    /// Lexical rules for the statement splitter and classifier.
    fn splitter_rules(&self) -> SplitterRules;

    /// The placeholder for bound parameter `index` (1-based) compared with a column of type
    /// `type_name`: `$1::text::integer` on PostgreSQL, `?` on MySQL, `?1` on SQLite. Drivers bind
    /// every parameter as the [`Value`](crate::Value) they are given.
    fn placeholder(&self, index: usize, type_name: &str) -> String;
}

/// Wraps `name` in `quote`, doubling every occurrence of `quote` inside it.
pub fn quote_with(name: &str, quote: char) -> String {
    let mut quoted = String::with_capacity(name.len() + 2);
    quoted.push(quote);
    for c in name.chars() {
        if c == quote {
            quoted.push(quote);
        }
        quoted.push(c);
    }
    quoted.push(quote);
    quoted
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::schema::NamespaceRef;

    struct Ansi;

    impl Dialect for Ansi {
        fn quote_identifier(&self, name: &str) -> String {
            quote_with(name, '"')
        }

        fn splitter_rules(&self) -> SplitterRules {
            SplitterRules::default()
        }

        fn placeholder(&self, index: usize, _type_name: &str) -> String {
            format!("${index}")
        }
    }

    #[rstest]
    #[case("users", "\"users\"")]
    #[case("my table", "\"my table\"")]
    #[case("a\"b", "\"a\"\"b\"")]
    #[case("\"", "\"\"\"\"")]
    #[case("", "\"\"")]
    #[case("Größe", "\"Größe\"")]
    fn quote_with_doubles_embedded_quotes(#[case] name: &str, #[case] expected: &str) {
        assert_eq!(quote_with(name, '"'), expected);
    }

    #[test]
    fn qualified_name_skips_missing_parts() {
        let table = TableRef {
            namespace: NamespaceRef {
                catalog: None,
                schema: Some("public".into()),
            },
            name: "orders".into(),
        };

        assert_eq!(Ansi.qualified_name(&table), "\"public\".\"orders\"");
    }

    #[test]
    fn qualified_name_quotes_every_part() {
        let table = TableRef {
            namespace: NamespaceRef {
                catalog: Some("d\"b".into()),
                schema: Some("s".into()),
            },
            name: "t".into(),
        };

        assert_eq!(Ansi.qualified_name(&table), "\"d\"\"b\".\"s\".\"t\"");
    }

    proptest::proptest! {
        #[test]
        fn quoted_identifier_unquotes_to_original(name in ".*") {
            let quoted = quote_with(&name, '"');

            let inner = &quoted[1..quoted.len() - 1];
            proptest::prop_assert_eq!(inner.replace("\"\"", "\""), name);
        }
    }
}
