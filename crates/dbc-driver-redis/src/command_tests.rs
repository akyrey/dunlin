use super::*;

fn args(line: &str) -> Vec<String> {
    parse_args(line)
        .unwrap()
        .into_iter()
        .map(|a| String::from_utf8(a).unwrap())
        .collect()
}

#[test]
fn arguments_follow_redis_cli_quoting() {
    assert_eq!(args("SET  user:1   hello"), ["SET", "user:1", "hello"]);
    assert_eq!(args(r#"SET k "a b\n\"c\"""#), ["SET", "k", "a b\n\"c\""]);
    assert_eq!(args(r"SET k 'it\'s \n'"), ["SET", "k", "it's \\n"]);
    assert_eq!(args(r#"SET k "\x41\x42""#), ["SET", "k", "AB"]);
    assert_eq!(args(r#"SET k """#), ["SET", "k", ""]);
    assert_eq!(args("SET k 😀"), ["SET", "k", "😀"]);
}

#[test]
fn unclosed_or_glued_quotes_are_refused() {
    assert_eq!(parse_args(r#"SET k "open"#), None);
    assert_eq!(parse_args(r#"SET k "a"b"#), None);
    assert_eq!(parse_args(r#"SET k "\xZZ""#), None);
}

#[test]
fn quoting_round_trips() {
    for value in [
        "plain",
        "with space",
        "q\"uote",
        "back\\slash",
        "",
        "line\nbreak",
        "it's",
    ] {
        let line = format!("SET {}", quote_arg(value));
        assert_eq!(args(&line)[1], value, "{line}");
    }
}

#[test]
fn every_line_is_a_command_and_comments_are_skipped() {
    let text = "SET a 1\n\n  # note\n  GET a  \r\n// also a note\nDEL a";
    let lines: Vec<&str> = split_lines(text)
        .iter()
        .map(|r| &text[r.start..r.end])
        .collect();

    assert_eq!(lines, ["SET a 1", "GET a", "DEL a"]);
}

#[test]
fn reads_writes_and_dangers_are_told_apart() {
    assert_eq!(classify("get a").class, StatementClass::Read);
    assert_eq!(classify("CONFIG GET maxmemory").class, StatementClass::Read);
    assert_eq!(
        classify("CONFIG SET maxmemory 1").class,
        StatementClass::Write
    );
    assert_eq!(classify("SET a 1").class, StatementClass::Write);
    assert_eq!(classify("SOMEMODULE.CMD x").class, StatementClass::Write);
    assert_eq!(classify("SELECT 2").class, StatementClass::Session);
    assert_eq!(classify("MULTI").class, StatementClass::Transaction);
    assert_eq!(classify("SUBSCRIBE ch").class, StatementClass::Unknown);
    assert!(classify("flushall").danger.is_some());
    assert!(classify("KEYS *").danger.is_some());
    assert!(classify("SCRIPT FLUSH").danger.is_some());
    assert!(classify("GET flushall").danger.is_none());
}

proptest::proptest! {
    #[test]
    fn no_text_panics(text in "\\PC{0,60}") {
        let _ = parse_args(&text);
        let _ = classify(&text);
        for range in split_lines(&text) {
            proptest::prop_assert!(text.get(range.start..range.end).is_some());
        }
    }
}
