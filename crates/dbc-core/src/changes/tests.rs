#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use super::*;
use crate::dialect::{quote_with, string_literal};
use crate::driver::CancelHandle;
use crate::schema::NamespaceRef;
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
    fn render_literal(&self, value: &Value, type_name: &str) -> String {
        match value {
            Value::Text(s) => format!("{}::{type_name}", string_literal(s)),
            other => crate::dialect::ansi_literal(other),
        }
    }
}

fn column(name: &str, type_name: &str) -> ColumnType {
    ColumnType {
        name: name.into(),
        type_name: type_name.into(),
    }
}

fn changes(operations: Vec<RowOperation>) -> ChangeSet {
    ChangeSet {
        table: TableRef {
            namespace: NamespaceRef {
                catalog: None,
                schema: Some("public".into()),
            },
            name: "order \"items\"".into(),
        },
        columns: vec![
            column("order_id", "integer"),
            column("line", "integer"),
            column("sku", "text"),
        ],
        operations,
    }
}

fn kv(name: &str, value: Value) -> (String, Value) {
    (name.into(), value)
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

const T: &str = r#""public"."order ""items""""#;

#[test]
fn statements_run_deletes_then_updates_then_inserts() {
    let set = changes(vec![
        RowOperation::Insert {
            values: vec![kv("order_id", Value::Int(1)), kv("sku", text("A"))],
        },
        RowOperation::Update {
            key: vec![kv("order_id", Value::Int(10)), kv("line", Value::Int(2))],
            set: vec![kv("line", Value::Int(3)), kv("sku", Value::Null)],
        },
        RowOperation::Delete {
            key: vec![kv("order_id", Value::Int(11)), kv("line", Value::Null)],
        },
        RowOperation::Insert { values: Vec::new() },
    ]);

    let planned = plan(&Pg, &set).unwrap();
    let executed: Vec<(usize, String, Vec<Value>, bool)> = planned
        .iter()
        .map(|s| {
            let bound = s.bound(&Pg);
            (s.operation, bound.sql, bound.params, s.one_row)
        })
        .collect();

    assert_eq!(
        executed,
        vec![
            (
                2,
                format!(
                    r#"DELETE FROM {T} WHERE "order_id" = $1::text::integer AND "line" IS NULL"#
                ),
                vec![Value::Int(11)],
                true
            ),
            (
                1,
                format!(
                    r#"UPDATE {T} SET "line" = $1::text::integer, "sku" = $2::text::text WHERE "order_id" = $3::text::integer AND "line" = $4::text::integer"#
                ),
                vec![Value::Int(3), Value::Null, Value::Int(10), Value::Int(2)],
                true
            ),
            (
                0,
                format!(
                    r#"INSERT INTO {T} ("order_id", "sku") VALUES ($1::text::integer, $2::text::text)"#
                ),
                vec![Value::Int(1), text("A")],
                false
            ),
            (
                3,
                format!("INSERT INTO {T} DEFAULT VALUES"),
                Vec::new(),
                false
            ),
        ]
    );
}

#[test]
fn preview_and_execution_come_from_the_same_parts() {
    let set = changes(vec![RowOperation::Update {
        key: vec![kv("order_id", Value::Int(10))],
        set: vec![kv("sku", text("'; DROP TABLE x; --"))],
    }]);

    let planned = plan(&Pg, &set).unwrap();
    let statement = &planned[0];
    let bound = statement.bound(&Pg);
    let preview = statement.preview(&Pg);

    // Replacing each placeholder by its rendered value turns the executed form into the preview.
    let values: Vec<&Part> = statement
        .parts
        .iter()
        .filter(|p| matches!(p, Part::Value { .. }))
        .collect();
    assert_eq!(values.len(), bound.params.len());
    let mut rebuilt = bound.sql.clone();
    for (i, part) in values.iter().enumerate().rev() {
        let Part::Value { value, type_name } = part else {
            unreachable!()
        };
        rebuilt = rebuilt.replace(
            &Pg.placeholder(i + 1, type_name),
            &Pg.render_literal(value, type_name),
        );
    }
    assert_eq!(rebuilt, preview);
    assert_eq!(
        preview,
        format!(r#"UPDATE {T} SET "sku" = '''; DROP TABLE x; --'::text WHERE "order_id" = 10"#)
    );
    assert_eq!(bound.params[0], text("'; DROP TABLE x; --"));
    assert_eq!(super::preview(&Pg, &set).unwrap(), vec![preview]);
}

#[test]
fn refuses_unknown_columns_missing_keys_and_empty_updates() {
    let unknown = changes(vec![RowOperation::Insert {
        values: vec![kv("nope\" --", Value::Int(1))],
    }]);
    let no_key = changes(vec![RowOperation::Delete { key: Vec::new() }]);
    let empty_update = changes(vec![RowOperation::Update {
        key: vec![kv("order_id", Value::Int(1))],
        set: Vec::new(),
    }]);

    for set in [unknown, no_key, empty_update] {
        assert_eq!(plan(&Pg, &set).unwrap_err().code, DbErrorCode::Execution);
    }
    assert!(plan(&Pg, &changes(Vec::new())).unwrap().is_empty());
}

/// Records statements; `affected` says how many rows each `query_params` call reports.
#[derive(Default)]
struct Recorder {
    log: Mutex<Vec<String>>,
    affected: Mutex<Vec<Result<u64, DbError>>>,
}

struct NoCancel;

#[async_trait]
impl CancelHandle for NoCancel {
    async fn cancel(&self) -> Result<(), DbError> {
        Ok(())
    }
}

#[async_trait]
impl Connection for Recorder {
    async fn ping(&self) -> Result<(), DbError> {
        Ok(())
    }
    async fn server_version(&self) -> Result<String, DbError> {
        Ok("1".into())
    }
    async fn execute(&self, sql: &str, _: &ExecuteOptions) -> Result<StatementOutcome, DbError> {
        self.log.lock().unwrap().push(sql.to_owned());
        Ok(StatementOutcome::Affected { rows: 0 })
    }
    async fn query_params(
        &self,
        sql: &str,
        _: &[Value],
        _: &ExecuteOptions,
    ) -> Result<StatementOutcome, DbError> {
        self.log
            .lock()
            .unwrap()
            .push(sql.split(' ').next().unwrap_or_default().to_owned());
        let next = self.affected.lock().unwrap().remove(0);
        next.map(|rows| StatementOutcome::Affected { rows })
    }
    fn cancel_handle(&self) -> Arc<dyn CancelHandle> {
        Arc::new(NoCancel)
    }
    async fn close(&self) -> Result<(), DbError> {
        Ok(())
    }
}

fn batch() -> ChangeSet {
    changes(vec![
        RowOperation::Insert {
            values: vec![kv("order_id", Value::Int(1))],
        },
        RowOperation::Delete {
            key: vec![kv("order_id", Value::Int(2))],
        },
    ])
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

#[test]
fn apply_commits_when_every_statement_succeeds() {
    let connection = Recorder::default();
    *connection.affected.lock().unwrap() = vec![Ok(1), Ok(1)];

    let report = block_on(apply(&connection, &Pg, &batch())).unwrap();

    assert_eq!(
        report,
        ApplyReport {
            statements: 2,
            rows_affected: 2
        }
    );
    assert_eq!(
        *connection.log.lock().unwrap(),
        vec!["BEGIN", "DELETE", "INSERT", "COMMIT"]
    );
}

#[test]
fn apply_rolls_back_when_a_row_is_gone_and_names_the_operation() {
    let connection = Recorder::default();
    *connection.affected.lock().unwrap() = vec![Ok(0)];

    let failure = block_on(apply(&connection, &Pg, &batch())).unwrap_err();

    assert_eq!(failure.operation, Some(1));
    assert_eq!(failure.error.code, DbErrorCode::RowCountMismatch);
    assert_eq!(
        *connection.log.lock().unwrap(),
        vec!["BEGIN", "DELETE", "ROLLBACK"]
    );
}

#[test]
fn apply_rolls_back_on_a_failing_statement() {
    let connection = Recorder::default();
    *connection.affected.lock().unwrap() = vec![
        Ok(1),
        Err(DbError::new(DbErrorCode::Constraint, "duplicate")),
    ];

    let failure = block_on(apply(&connection, &Pg, &batch())).unwrap_err();

    assert_eq!(failure.operation, Some(0));
    assert_eq!(failure.error.code, DbErrorCode::Constraint);
    assert_eq!(
        *connection.log.lock().unwrap(),
        vec!["BEGIN", "DELETE", "INSERT", "ROLLBACK"]
    );
}

#[test]
fn apply_refuses_an_invalid_plan_before_beginning() {
    let connection = Recorder::default();
    let bad = changes(vec![RowOperation::Delete { key: Vec::new() }]);

    let failure = block_on(apply(&connection, &Pg, &bad)).unwrap_err();

    assert_eq!(failure.operation, None);
    assert!(connection.log.lock().unwrap().is_empty());
}
