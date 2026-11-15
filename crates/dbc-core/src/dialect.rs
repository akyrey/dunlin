//! SQL dialect: quoting and lexical rules.

use crate::schema::TableRef;
use crate::sql::SplitterRules;
use crate::value::Value;

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

    /// The statements of a script. SQL splits on `;` (with the dialect's rules); a command
    /// language can split differently (Redis: one command per line).
    fn split(&self, text: &str) -> Vec<crate::sql::StatementRange> {
        crate::sql::split(text, self.splitter_rules())
    }

    /// What one statement does, for read-only sessions and confirmations.
    fn classify(&self, statement: &str) -> crate::sql::Classification {
        crate::sql::classify(statement, self.splitter_rules())
    }

    /// How the database stores an unquoted identifier: PostgreSQL lowercases it; the others
    /// keep it as written.
    fn fold_identifier(&self, name: &str) -> String {
        name.to_owned()
    }

    /// The placeholder for bound parameter `index` (1-based) compared with a column of type
    /// `type_name`: `$1::text::integer` on PostgreSQL, `?` on MySQL, `?1` on SQLite. Drivers bind
    /// every parameter as the [`Value`](crate::Value) they are given.
    fn placeholder(&self, index: usize, type_name: &str) -> String;

    /// A literal for `value` in a column of type `type_name`, for the change preview and `INSERT`
    /// export only: the app never executes it (SECURITY.md#generated-sql). The default is ANSI
    /// SQL with `X'..'` for binary.
    fn render_literal(&self, value: &Value, _type_name: &str) -> String {
        ansi_literal(value)
    }

    /// What follows `INSERT INTO t` when every column takes its default.
    fn default_values(&self) -> &'static str {
        "DEFAULT VALUES"
    }
}

/// `'it''s'`: single quotes doubled, nothing else escaped (standard SQL strings).
pub fn string_literal(text: &str) -> String {
    quote_with(text, '\'')
}

/// `text` when it is a plain decimal number (`-12.50`, `3e10`), so it can be written unquoted.
pub fn numeric_literal(text: &str) -> Option<&str> {
    let unsigned = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((m, e)) => (m, Some(e.strip_prefix(['-', '+']).unwrap_or(e))),
        None => (unsigned, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let valid = !(whole.is_empty() && fraction.is_empty())
        && digits(whole)
        && digits(fraction)
        && exponent.is_none_or(|e| !e.is_empty() && digits(e));
    valid.then_some(text)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The ANSI form of every value; dialects override what they spell differently.
pub fn ansi_literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_owned(),
        Value::Int(i) => i.to_string(),
        Value::UInt(u) => u.to_string(),
        Value::Float(f) if f.is_finite() => f.to_string(),
        Value::Float(f) => string_literal(&f.to_string()),
        Value::Decimal(text) => {
            numeric_literal(text).map_or_else(|| string_literal(text), str::to_owned)
        }
        Value::Bytes(bytes) => format!("X'{}'", hex(bytes)),
        Value::Text(s)
        | Value::Json(s)
        | Value::Xml(s)
        | Value::Uuid(s)
        | Value::Date(s)
        | Value::Time(s)
        | Value::Timestamp(s)
        | Value::TimestampTz(s)
        | Value::Interval(s) => string_literal(s),
        Value::Array(items) => format!(
            "ARRAY[{}]",
            items
                .iter()
                .map(ansi_literal)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Unknown {
            text: Some(text), ..
        } => string_literal(text),
        Value::Unknown { hex: Some(hex), .. } => format!("X'{hex}'"),
        Value::Unknown { .. } => "NULL".to_owned(),
    }
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
    use crate::value::Value;

    #[test]
    fn ansi_literals_for_every_value() {
        let cases = [
            (Value::Null, "NULL"),
            (Value::Bool(true), "TRUE"),
            (Value::Bool(false), "FALSE"),
            (Value::Int(-3), "-3"),
            (Value::UInt(u64::MAX), "18446744073709551615"),
            (Value::Float(2.5), "2.5"),
            (Value::Float(f64::INFINITY), "'inf'"),
            (Value::Decimal("12.50".into()), "12.50"),
            (Value::Decimal("NaN".into()), "'NaN'"),
            (Value::Bytes(vec![0, 255]), "X'00ff'"),
            (Value::Text("it's".into()), "'it''s'"),
            (Value::Date("2026-01-02".into()), "'2026-01-02'"),
            (
                Value::Array(vec![Value::Int(1), Value::Null]),
                "ARRAY[1, NULL]",
            ),
            (
                Value::Unknown {
                    type_name: "point".into(),
                    text: Some("(1,2)".into()),
                    hex: None,
                },
                "'(1,2)'",
            ),
            (
                Value::Unknown {
                    type_name: "blob".into(),
                    text: None,
                    hex: Some("abcd".into()),
                },
                "X'abcd'",
            ),
            (
                Value::Unknown {
                    type_name: "void".into(),
                    text: None,
                    hex: None,
                },
                "NULL",
            ),
        ];
        for (value, expected) in cases {
            assert_eq!(ansi_literal(&value), expected, "{value:?}");
        }
    }

    #[rstest]
    #[case("-12.50", true)]
    #[case("+3e10", true)]
    #[case("1E-5", true)]
    #[case(".5", true)]
    #[case("5.", true)]
    #[case(".", false)]
    #[case("1e", false)]
    #[case("0x10", false)]
    #[case("1,5", false)]
    fn numeric_literals_are_plain_decimal_numbers(#[case] text: &str, #[case] plain: bool) {
        assert_eq!(numeric_literal(text).is_some(), plain, "{text}");
    }

    #[test]
    fn dialect_defaults_follow_standard_sql() {
        let dialect = Ansi;

        assert_eq!(dialect.split("SELECT 1; SELECT 2").len(), 2);
        assert!(dialect.classify("SELECT 1").class == crate::sql::StatementClass::Read);
        assert_eq!(dialect.fold_identifier("Name"), "Name");
        assert_eq!(dialect.render_literal(&Value::Int(1), "int"), "1");
        assert_eq!(dialect.default_values(), "DEFAULT VALUES");
    }

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
