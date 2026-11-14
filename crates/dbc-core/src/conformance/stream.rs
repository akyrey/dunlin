//! Streaming conformance (task 8.3): rows arrive in batches, and a reader that stops early
//! leaves the connection usable.

use tokio::sync::mpsc;

use crate::result::{ExecuteOptions, STREAM_BATCH, StreamEvent};

use super::{Failure, Harness, ensure, fail};

/// 2500 rows on every database (MySQL caps CTE recursion at 1000 levels, so two small ones).
const MANY_ROWS: &str = "WITH RECURSIVE a(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM a WHERE n < 50) \
                         SELECT x.n * 100 + y.n AS v FROM a x CROSS JOIN a y";
const MANY: u64 = 2500;

pub async fn stream_group(harness: &dyn Harness) -> Result<(), Failure> {
    let connection = harness
        .driver()
        .connect(&harness.database(false).await)
        .await
        .map_err(|e| fail("connect", e.to_string()))?;

    let (tx, mut rx) = mpsc::channel(4);
    let reader = async {
        let mut batches = Vec::new();
        let mut columns = 0;
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Columns(c) => columns = c.len(),
                StreamEvent::Rows(rows) => batches.push(rows.len()),
            }
        }
        (columns, batches)
    };
    let (sent, (columns, batches)) =
        tokio::join!(connection.stream_rows(MANY_ROWS, &[], tx), reader);
    let sent = sent.map_err(|e| fail("stream", e.to_string()))?;
    ensure(
        sent == MANY
            && columns == 1
            && batches.iter().all(|b| *b <= STREAM_BATCH)
            && batches.iter().sum::<usize>() == 2500,
        "stream",
        format!("sent {sent}, columns {columns}, batches {batches:?}"),
    )?;

    // A reader that stops after the first batch.
    let (tx, mut rx) = mpsc::channel(1);
    let reader = async {
        let mut rows = 0;
        while let Some(event) = rx.recv().await {
            if let StreamEvent::Rows(batch) = event {
                rows += batch.len();
                break;
            }
        }
        drop(rx);
        rows
    };
    let (stopped, read) = tokio::join!(connection.stream_rows(MANY_ROWS, &[], tx), reader);
    let stopped = stopped.map_err(|e| fail("stopped stream", e.to_string()))?;
    ensure(
        read == STREAM_BATCH && stopped < MANY,
        "stopped stream",
        format!("read {read}, sent {stopped}"),
    )?;
    let after = connection
        .execute("SELECT 1", &ExecuteOptions::default())
        .await;
    ensure(
        after.is_ok(),
        "stopped stream",
        format!("connection unusable afterwards: {after:?}"),
    )?;
    connection
        .close()
        .await
        .map_err(|e| fail("close", e.to_string()))
}
