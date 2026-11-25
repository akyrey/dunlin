# Phase 10: Query performance

## Goal

Show where a query spends its time and why, building on the visual `EXPLAIN` (D-029): measured plans, hints on the plan with an index suggestion for the simple cases, and the tables' and server's own statistics.

## Read first

DECISIONS D-029, D-043, [DRIVER_API.md](../DRIVER_API.md#capabilities), `dbc_core::plan`, [Phase 9](phase-9-schema-editing.md) (the editor an index suggestion opens).

## In scope

1. **Measured plans.** An "Explain analyze" action next to Explain. It runs the statement and shows actual rows, loops and time per node next to the estimates.
2. **Plan hints.** Marks on the plan tree, computed in `dbc-core` from `PlanNode`. A hint for a simple case offers "Create index…", which opens the table's detail tab in edit mode with the index staged.
3. **Table statistics** in the detail tab.
4. **Server query statistics:** the heaviest statements, where the server records them.

## Out of scope

Repeated timing ("run N times", min / median / max): `EXPLAIN ANALYZE` covers the need for now; revisit if it is missed. Index suggestions for anything beyond the simple case below. Automatic tuning, `VACUUM`/`ANALYZE`/`OPTIMIZE` actions, plan comparison between runs, plan history.

## Measured plans

| | PostgreSQL | MySQL | MariaDB | SQLite |
|---|---|---|---|---|
| Statement | `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` | `EXPLAIN ANALYZE` (TREE text), 8.0.18+ | `ANALYZE FORMAT=JSON` | none per node: the plan from `EXPLAIN QUERY PLAN` plus the statement's total time |
| Writes | run inside `BEGIN` … `ROLLBACK` | not offered | not offered | run inside `BEGIN` … `ROLLBACK`, total time only |
| Per node | actual rows, loops, total time, shared hits/reads, sort/hash spill | actual rows, loops, time to first and last row | `r_rows`, `r_loops`, `r_total_time_ms` | none |

Checked on PostgreSQL 18, MySQL 8.4 and MariaDB 11.4 (10.1); the captured outputs are the parser fixtures:

- PostgreSQL and MySQL report rows and time **per loop**; `PlanNode::actual_time_ms` is per loop × loops. PostgreSQL's `Execution Time` is the total.
- MySQL writes small times in exponent form (`313e-6`) and marks steps that never ran `(never executed)`. Its filter is a `Filter:` step above the scan.
- MariaDB reports each table's own time (`r_table_time_ms` + `r_other_time_ms`, over all loops) and rows read (`r_rows`) with the share kept (`r_filtered`, percent); nested loops have no time of their own and take their tables' sum.
- MySQL and MariaDB add conditions the optimizer derived from joins to the filter (`o.cid is not null`). The index suggestion ignores `IS NOT NULL` conditions for that reason.
- Each driver marks full scans and names the scanned table and its filter in `PlanNode` (`full_scan`, `table`, `filter`, `rows_read`, `spilled`), so hints never guess from operation names. Plain Explain fills them too, so hints work on estimated plans.

Rules:

- The statement really runs, so Explain analyze goes through the same safeguards as running it: read-only sessions allow it for reads only; a dangerous statement asks for confirmation first; a statement that writes is only offered where it can be rolled back, and always is.
- It runs on the tab's connection, like Explain, and can be cancelled like a run. In a manual transaction (D-028) a write is refused: the rollback would end the user's transaction.
- Capability `explain_analyze` gates the action. The frontend never branches on the driver.

## Plan hints

`dbc_core::plan::hints(&PlanNode) -> Vec<PlanHint>`, pure, the same for every server. A hint names the node (its path in the tree), a kind and a sentence.

| Hint | When |
|---|---|
| `hottest` | The node with the largest own time (measured) or own cost (estimated) |
| `misestimate` | Actual rows differ from the estimate by 10x or more, both ways (measured only) |
| `full_scan` | A sequential or full table scan reading 10 000 rows or more (actual, else estimated) |
| `spill` | A sort or hash that wrote to disk (PostgreSQL measured) |
| `many_loops` | An inner side of a nested loop run 1 000 times or more |

Thresholds are named constants in `dbc_core::plan`.

### Index suggestion (simple case only)

A `full_scan` hint carries a suggested index when, and only when:

- the scanned node names one table of the current database;
- its filter is a conjunction of conditions on **one** column of that table: `col = …`, `col IN (…)`, `col < / <= / > / >= …`, `col BETWEEN … AND …`, `col IS NULL` (no functions or casts on the column, no `OR`);
- no existing index of that table starts with that column.

The suggestion is `Index { name: "<table>_<column>_idx", columns: [column], unique: false }`. Filters come from each server's plan (`Filter` on PostgreSQL, the `Filter:` step on MySQL, `attached_condition` on MariaDB), parsed with the shared lexer. SQLite plans carry no filter text, so SQLite gets hints but no suggestion. MySQL and MariaDB name a table by its alias when the query gives one; the backend checks the suggested table exists, so an aliased table gets no suggestion. Anything else gets the hint without a suggestion. "Create index…" stages an `AddIndex` in the table's detail tab (Phase 9); nothing runs until the user applies it there.

## Table statistics

A "Statistics" sub-tab of the table detail tab. `Connection::table_stats(&TableRef) -> TableStats`, every field optional:

| | PostgreSQL | MySQL / MariaDB | SQLite |
|---|---|---|---|
| Size on disk (table, indexes) | `pg_total_relation_size`, `pg_indexes_size` | `information_schema.tables` `data_length`, `index_length` | `dbstat` when compiled in, else none |
| Estimated rows | `reltuples` | `table_rows` | none |
| Per index: size, scans | `pg_stat_user_indexes.idx_scan`, `pg_relation_size` | size from `mysql.innodb_index_stats` | none |
| Maintenance | last (auto)vacuum and (auto)analyze, dead tuples | `update_time` | none |

An index with zero scans since the statistics were reset is marked "unused"; the tab says since when, where the server knows.

## Server query statistics

A "Query statistics" view per session (palette and session menu): the 50 statements with the most total time, with calls, total and mean time, rows. `Connection::query_stats() -> Option<Vec<QueryStat>>`:

- PostgreSQL: `pg_stat_statements` when the extension is installed and readable; otherwise a message saying how to enable it.
- MySQL and MariaDB: `performance_schema.events_statements_summary_by_digest` when `performance_schema` is on.
- SQLite: none (`None`, the view is not offered).

Statement text is shown as the server normalised it; selecting one opens it in a new editor tab.

## Tasks

- [x] 10.1 Spike: capture `EXPLAIN ANALYZE` output for the fixture queries on each server CI runs; record the real field names and fix this brief where it is wrong.
- [x] 10.2 `PlanNode` gains `actual_rows`, `loops`, `actual_time_ms` (total, inclusive), `details` keeps buffers and spill notes; `Capabilities::explain_analyze`. Generated types updated.
- [x] 10.3 PostgreSQL: `explain_analyze` with rollback for writes; parser tests from captured JSON.
- [x] 10.4 MySQL (TREE text) and MariaDB (`ANALYZE FORMAT=JSON`) parsers, reads only; tests per flavour.
- [x] 10.5 SQLite: plan plus total time, writes rolled back.
- [ ] 10.6 `query_explain_analyze` command: read-only and danger checks, refusal of writes in a manual transaction, cancellation. Command tests.
- [ ] 10.7 `dbc_core::plan::hints` with unit tests for every hint and threshold edge.
- [ ] 10.8 Filter parsing and the index suggestion, with tests for each accepted condition form and each rejection (two columns, `OR`, function on the column, already indexed, unknown table).
- [ ] 10.9 Frontend: Explain analyze action, measured columns in the plan view, hint badges, "Create index…" staging an `AddIndex` in the detail tab.
- [ ] 10.10 `table_stats` per driver and the Statistics sub-tab, with unused indexes marked.
- [ ] 10.11 `query_stats` per driver and the Query statistics view, with the not-enabled messages.
- [ ] 10.12 Conformance group "explain analyze": a read returns measured numbers where supported; a write is rolled back (row count unchanged); read-only refuses writes.
- [ ] 10.13 E2E: explain analyze a read, see measured numbers and a hint; create the suggested index through the preview.

## Acceptance criteria

- Explain analyze shows actual rows and times on PostgreSQL, MySQL 8.0.18+ and MariaDB, and total time on SQLite.
- A write explained with analyze never changes data (conformance group).
- Hints match their definitions on the unit-test plans; the index suggestion appears only for the simple case.
- Table statistics show sizes on PostgreSQL and MySQL and mark unused indexes on PostgreSQL.
- The query statistics view lists statements where the server records them and explains how to enable it where it does not.

## Manual smoke

1. On a real database, Explain analyze a slow report query; follow the hottest node.
2. Accept an index suggestion, preview it, apply it, and Explain analyze again.
3. Open Query statistics on a server with `pg_stat_statements` and with `performance_schema`.
