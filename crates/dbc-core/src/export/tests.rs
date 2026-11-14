#![allow(clippy::unwrap_used)]

use super::*;
use crate::dialect::{ansi_literal, quote_with};
use crate::result::TypeCategory;
use crate::schema::NamespaceRef;
use crate::sql::SplitterRules;

struct Ansi;

impl Dialect for Ansi {
    fn quote_identifier(&self, name: &str) -> String {
        quote_with(name, '"')
    }
    fn splitter_rules(&self) -> SplitterRules {
        SplitterRules::default()
    }
    fn placeholder(&self, index: usize, _: &str) -> String {
        format!("${index}")
    }
}

fn columns(names: &[&str]) -> Vec<ColumnMeta> {
    names
        .iter()
        .map(|name| ColumnMeta {
            name: (*name).into(),
            type_name: "text".into(),
            category: TypeCategory::Text,
            nullable: None,
        })
        .collect()
}

fn awkward() -> Vec<Vec<Value>> {
    vec![
        vec![
            Value::Int(1),
            Value::Text("say \"hi\", then\nleave".into()),
            Value::Null,
            Value::Bytes(vec![0xde, 0xad]),
        ],
        vec![
            Value::Int(9_007_199_254_740_993),
            Value::Text("naïve 😀;tab\t".into()),
            Value::Text(String::new()),
            Value::Json("{\"big\": 12345678901234567890}".into()),
        ],
    ]
}

fn export(format: ExportFormat, batches: &[Vec<Vec<Value>>]) -> String {
    let mut writer = ExportWriter::new(Vec::new(), format);
    writer
        .begin(&columns(&["id", "note, \"quoted\"", "empty", "data"]))
        .unwrap();
    for batch in batches {
        writer.rows(batch, &Ansi).unwrap();
    }
    assert_eq!(
        writer.rows_written(),
        batches.iter().map(|b| b.len() as u64).sum::<u64>()
    );
    String::from_utf8(writer.finish().unwrap()).unwrap()
}

#[test]
fn csv_follows_rfc_4180() {
    let out = export(ExportFormat::Csv(CsvOptions::default()), &[awkward()]);

    assert_eq!(
        out,
        "id,\"note, \"\"quoted\"\"\",empty,data\r\n\
         1,\"say \"\"hi\"\", then\nleave\",,\\xdead\r\n\
         9007199254740993,naïve 😀;tab\t,\"\",\"{\"\"big\"\": 12345678901234567890}\"\r\n"
    );
}

#[test]
fn csv_options_change_delimiter_header_and_null() {
    let options = CsvOptions {
        delimiter: ';',
        header: false,
        null: "\\N".into(),
    };

    let out = export(ExportFormat::Csv(options), &[awkward()]);

    assert!(out.starts_with("1;\"say \"\"hi\"\", then\nleave\";\\N;\\xdead\r\n"));
    assert!(out.contains("\"naïve 😀;tab\t\""));

    let null_word = CsvOptions {
        null: "NULL".into(),
        ..CsvOptions::default()
    };
    let out = export(
        ExportFormat::Csv(null_word),
        &[vec![vec![
            Value::Text("NULL".into()),
            Value::Null,
            Value::Int(1),
            Value::Int(2),
        ]]],
    );
    assert!(out.ends_with("\"NULL\",NULL,1,2\r\n"), "{out}");
}

#[test]
fn json_keeps_exact_numbers_and_embeds_documents() {
    let out = export(ExportFormat::Json, &[awkward()]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(parsed[0]["id"], 1);
    assert_eq!(parsed[0]["data"], "3q0=");
    assert_eq!(parsed[0]["empty"], serde_json::Value::Null);
    assert_eq!(parsed[1]["id"], "9007199254740993");
    assert!(
        out.contains("{\"big\": 12345678901234567890}"),
        "the document is embedded verbatim"
    );
    assert_eq!(export(ExportFormat::Json, &[]), "[]\n");
}

#[test]
fn json_numbers() {
    let numbers = vec![vec![
        Value::Decimal("12.50".into()),
        Value::Decimal("12345678901234567890.123456789".into()),
        Value::Float(1.5),
        Value::Float(f64::NAN),
    ]];
    let mut writer = ExportWriter::new(Vec::new(), ExportFormat::Json);
    writer.begin(&columns(&["a", "b", "c", "d"])).unwrap();
    writer.rows(&numbers, &Ansi).unwrap();
    let out = String::from_utf8(writer.finish().unwrap()).unwrap();

    assert!(out.contains("\"a\": 12.5,"), "{out}");
    assert!(
        out.contains("\"b\": \"12345678901234567890.123456789\""),
        "{out}"
    );
    assert!(out.contains("\"c\": 1.5"), "{out}");
    assert!(out.contains("\"d\": \"NaN\""), "{out}");
}

#[test]
fn inserts_are_batched_with_literals() {
    let table = TableRef {
        namespace: NamespaceRef {
            catalog: None,
            schema: Some("main".into()),
        },
        name: "t".into(),
    };
    let rows = awkward();

    let out = export(
        ExportFormat::Insert { table, batch: 1 },
        &[rows.clone(), rows[..1].to_vec()],
    );

    let head = r#"INSERT INTO "main"."t" ("id", "note, ""quoted""", "empty", "data") VALUES"#;
    assert_eq!(out.matches(head).count(), 3);
    assert!(out.contains(&format!(
        "({}, {}, NULL, X'dead');",
        1,
        ansi_literal(&rows[0][1])
    )));
}

#[test]
fn arrays_and_unknown_values_have_text_forms() {
    let row = vec![vec![
        Value::Array(vec![Value::Text("a b".into()), Value::Null, Value::Int(1)]),
        Value::Unknown {
            type_name: "x".into(),
            text: None,
            hex: Some("ff".into()),
        },
        Value::Unknown {
            type_name: "x".into(),
            text: None,
            hex: None,
        },
        Value::Bool(true),
    ]];

    let out = export(
        ExportFormat::Csv(CsvOptions {
            header: false,
            ..CsvOptions::default()
        }),
        &[row],
    );

    assert_eq!(out, "\"{\"\"a b\"\",NULL,1}\",\\xff,,true\r\n");
}
