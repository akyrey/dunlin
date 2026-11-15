#![allow(clippy::unwrap_used)]

use super::*;
use crate::dialect::quote_with;
use crate::result::TypeCategory;
use crate::schema::{Column, NamespaceRef};
use crate::sql::SplitterRules;

struct Pg;

impl Dialect for Pg {
    fn quote_identifier(&self, name: &str) -> String {
        quote_with(name, '"')
    }
    fn splitter_rules(&self) -> SplitterRules {
        SplitterRules::default()
    }
    fn placeholder(&self, index: usize, type_name: &str) -> String {
        format!("${index}::text::{type_name}")
    }
}

fn options(delimiter: char, header: bool, null: &str) -> CsvOptions {
    CsvOptions {
        delimiter,
        header,
        null: null.into(),
    }
}

fn records(text: &str, delimiter: char) -> Vec<Result<Record, ImportError>> {
    CsvRecords::new(text.as_bytes(), delimiter).collect()
}

fn texts(record: &Record) -> Vec<(&str, bool)> {
    record
        .fields
        .iter()
        .map(|f| (f.text.as_str(), f.quoted))
        .collect()
}

#[test]
fn reads_plain_and_quoted_fields_with_their_line_numbers() {
    let text = "\u{feff}id,name\r\n1,\"Ada, Countess\"\n\n2,\"say \"\"hi\"\"\nthere\",\n3,\"\"\n";

    let read: Vec<Record> = records(text, ',').into_iter().map(Result::unwrap).collect();

    assert_eq!(texts(&read[0]), vec![("id", false), ("name", false)]);
    assert_eq!(texts(&read[1]), vec![("1", false), ("Ada, Countess", true)]);
    assert_eq!(
        texts(&read[2]),
        vec![("2", false), ("say \"hi\"\nthere", true), ("", false)]
    );
    assert_eq!(texts(&read[3]), vec![("3", false), ("", true)]);
    let lines: Vec<u64> = read.iter().map(|r| r.line).collect();
    assert_eq!(
        lines,
        vec![1, 2, 4, 6],
        "blank lines are skipped, not counted as rows"
    );
}

#[test]
fn other_delimiters_and_a_last_line_without_a_break() {
    let read: Vec<Record> = records("a\tb\n1\t2", '\t')
        .into_iter()
        .map(Result::unwrap)
        .collect();

    assert_eq!(texts(&read[1]), vec![("1", false), ("2", false)]);
}

#[test]
fn malformed_quotes_are_errors_with_the_line() {
    let stray = records("a,b\n1,5\" screen\n", ',');
    let after = records("\"a\"b,c\n", ',');
    let open = records("x\n\"never closed\n", ',');

    let message = |r: &[Result<Record, ImportError>]| {
        r.iter().find_map(|r| r.as_ref().err()).unwrap().to_string()
    };
    assert!(
        message(&stray).starts_with("Line 2: "),
        "{}",
        message(&stray)
    );
    assert!(message(&after).starts_with("Line 1: "));
    assert!(message(&open).starts_with("Line 2: "));
}

#[test]
fn invalid_utf8_is_an_error_with_the_line() {
    let bytes: &[u8] = b"a\n\xff\xfe\n";

    let read: Vec<_> = CsvRecords::new(bytes, ',').collect();

    assert_eq!(
        read[1].as_ref().unwrap_err().to_string(),
        "Line 2: the file is not UTF-8 text"
    );
}

fn column(name: &str, category: TypeCategory) -> Column {
    Column {
        name: name.into(),
        type_name: "t".into(),
        category,
        nullable: true,
        default: None,
        comment: None,
        ordinal: 1,
        generated: false,
    }
}

fn field(text: &str, quoted: bool) -> Field {
    Field {
        text: text.into(),
        quoted,
    }
}

#[test]
fn fields_become_values_of_the_column_type() {
    let int = column("n", TypeCategory::Integer);
    let cases = [
        (
            field("", false),
            column("s", TypeCategory::Text),
            Value::Null,
        ),
        (
            field("", true),
            column("s", TypeCategory::Text),
            Value::Text(String::new()),
        ),
        (field(" 42 ", false), int.clone(), Value::Int(42)),
        (
            field("18446744073709551615", false),
            int.clone(),
            Value::UInt(u64::MAX),
        ),
        (
            field("2.5", false),
            column("f", TypeCategory::Float),
            Value::Float(2.5),
        ),
        (
            field("1.10", false),
            column("d", TypeCategory::Decimal),
            Value::Decimal("1.10".into()),
        ),
        (
            field("TRUE", false),
            column("b", TypeCategory::Bool),
            Value::Bool(true),
        ),
        (
            field("0", false),
            column("b", TypeCategory::Bool),
            Value::Bool(false),
        ),
        (
            field("\\x00ff", false),
            column("y", TypeCategory::Binary),
            Value::Bytes(vec![0, 255]),
        ),
        (
            field("hi", false),
            column("y", TypeCategory::Binary),
            Value::Bytes(b"hi".to_vec()),
        ),
        (
            field("2026-01-02", false),
            column("t", TypeCategory::Temporal),
            Value::Text("2026-01-02".into()),
        ),
    ];

    for (input, target, expected) in cases {
        assert_eq!(
            field_value(&input, &target, "").unwrap(),
            expected,
            "{input:?}"
        );
    }
    assert_eq!(
        field_value(&field("NULL", false), &int, "NULL").unwrap(),
        Value::Null
    );
    assert_eq!(
        field_value(
            &field("NULL", true),
            &column("s", TypeCategory::Text),
            "NULL"
        )
        .unwrap(),
        Value::Text("NULL".into())
    );
}

#[test]
fn values_that_do_not_fit_the_column_are_refused() {
    let refused = [
        (field("4x", false), column("n", TypeCategory::Integer)),
        (field("1.5", false), column("n", TypeCategory::Integer)),
        (field("abc", false), column("f", TypeCategory::Float)),
        (field("maybe", false), column("b", TypeCategory::Bool)),
        (field("\\xZZ", false), column("y", TypeCategory::Binary)),
    ];

    for (input, target) in refused {
        assert!(field_value(&input, &target, "").is_err(), "{input:?}");
    }
}

fn target() -> ImportTarget {
    ImportTarget {
        table: TableRef {
            namespace: NamespaceRef {
                catalog: None,
                schema: Some("public".into()),
            },
            name: "people".into(),
        },
        columns: vec![
            (column("id", TypeCategory::Integer), 1),
            (column("name", TypeCategory::Text), 0),
        ],
    }
}

#[test]
fn rows_insert_in_one_statement_with_bound_values() {
    let rows = vec![
        vec![Value::Int(1), Value::Text("Ada".into())],
        vec![Value::Int(2), Value::Null],
    ];

    let bound = insert_sql(&Pg, &target(), &rows);

    assert_eq!(
        bound.sql,
        r#"INSERT INTO "public"."people" ("id", "name") VALUES ($1::text::t, $2::text::t), ($3::text::t, $4::text::t)"#
    );
    assert_eq!(bound.params, rows.concat());
}

#[test]
fn a_record_maps_onto_the_target_columns_or_names_its_line() {
    let record = Record {
        line: 7,
        fields: vec![field("Ada", false), field("1", false)],
    };
    let bad = Record {
        line: 8,
        fields: vec![field("Bob", false), field("one", false)],
    };

    assert_eq!(
        row_values(&target(), &record, "").unwrap(),
        vec![Value::Int(1), Value::Text("Ada".into())]
    );
    let error = row_values(&target(), &bad, "").unwrap_err().to_string();
    assert_eq!(error, "Line 8, column id: \"one\" is not a whole number");
}

#[test]
fn batches_hold_whole_statements_under_the_parameter_limit() {
    assert_eq!(rows_per_statement(2), MAX_ROWS_PER_STATEMENT);
    assert_eq!(rows_per_statement(1000), MAX_PARAMETERS / 1000);
    assert_eq!(rows_per_statement(100_000), 1);
}

#[test]
fn the_reader_skips_the_header_checks_widths_and_sends_batches() {
    let text = "name,id\nAda,1\nBob,2\nCy,3\n";
    let mut sent = Vec::new();

    let header = read_batches(text.as_bytes(), &options(',', true, ""), 2, 2, |batch| {
        sent.push(batch.iter().map(|r| r.line).collect::<Vec<_>>());
        true
    })
    .unwrap();

    assert_eq!(header, Some(vec!["name".to_owned(), "id".to_owned()]));
    assert_eq!(sent, vec![vec![2, 3], vec![4]]);
}

#[test]
fn the_reader_stops_on_a_ragged_row_or_too_few_columns() {
    let ragged = read_batches(
        "a,b\n1,2\n3\n".as_bytes(),
        &options(',', false, ""),
        2,
        10,
        |_| true,
    );
    let narrow = read_batches("a\n".as_bytes(), &options(',', false, ""), 2, 10, |_| true);

    assert_eq!(
        ragged.unwrap_err().to_string(),
        "Line 3: 1 field where the first row has 2"
    );
    assert_eq!(
        narrow.unwrap_err().to_string(),
        "Line 1: the file has 1 column, but the mapping uses column 2"
    );
}

#[test]
fn the_reader_stops_when_the_receiver_goes_away() {
    let text = "1\n2\n3\n4\n";
    let mut calls = 0;

    read_batches(text.as_bytes(), &options(',', false, ""), 1, 1, |_| {
        calls += 1;
        false
    })
    .unwrap();

    assert_eq!(calls, 1);
}

#[test]
fn a_preview_names_unnamed_columns_and_shows_nulls() {
    let preview = preview("1,,x\n2,\"\",y\n".as_bytes(), &options(',', false, ""), 10).unwrap();

    assert_eq!(preview.headers, vec!["Column 1", "Column 2", "Column 3"]);
    assert_eq!(
        preview.rows[1],
        vec![
            Some("2".to_owned()),
            Some(String::new()),
            Some("y".to_owned())
        ]
    );
    assert_eq!(preview.rows[0][1], None);
}
