# Phase 10: Query performance

## Goal

Show where a query spends its time and why, building on the visual `EXPLAIN` (D-029): measured plans, hints on the plan, and the tables' and server's own statistics.

## Read first

DECISIONS D-029, [DRIVER_API.md](../DRIVER_API.md#capabilities), `dbc_core::plan`.

## Status

Draft. The scope below is a proposal; confirm it before the tasks are written.

## Proposed scope

1. **Measured plans (`EXPLAIN ANALYZE`).** A second action next to Explain that runs the statement and shows actual rows, loops and time per node next to the estimates. PostgreSQL `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)`; MySQL 8.0.18+ `EXPLAIN ANALYZE`; MariaDB `ANALYZE FORMAT=JSON`; SQLite has no per-node timing, so it shows the plan and the total time. The statement really runs: a statement that writes asks for confirmation and runs inside a transaction that is rolled back where the database allows it (PostgreSQL, SQLite); on MySQL only reads are offered. Read-only sessions allow it for reads only. This supersedes D-029's "never ANALYZE".
2. **Plan hints.** Marks on the plan tree, computed in `dbc-core` from `PlanNode`: the node with the most time (or cost), row estimates off by 10x or more, full scans of large tables, sorts and hashes that spilled to disk, nested loops with many iterations.
3. **Table statistics** in the detail tab: size on disk, estimated rows, index sizes, and on PostgreSQL index usage (`pg_stat_user_indexes`, so unused indexes are visible), last `VACUUM` / `ANALYZE`, and dead tuples.
4. **Server query statistics.** A per-session view of the heaviest statements: PostgreSQL `pg_stat_statements` (when the extension is installed), MySQL `performance_schema` statement digests. Not available on SQLite.

## Open questions

- Is a "run N times" timing mode (min / median / max) worth it, or is `EXPLAIN ANALYZE` enough?
- Should hints suggest an index (with a "create index" action that opens the Phase 9 editor), or only point at the problem?
