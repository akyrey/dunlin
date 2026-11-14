# Phase 6: Data viewing

## Goal

Open any table as a paged, sortable, filterable grid; switch any result between grid and JSON; inspect JSON, XML, long text and binary cells in a viewer panel; follow foreign keys.

## Read first

PRD FR-4, [DRIVER_API.md](../DRIVER_API.md) (results, dialect), DECISIONS D-006.

## In scope

- `fetch_table_page` and `count_rows` in all three drivers.
- Table data tab, used from the schema tree.
- Grid upgrades shared by table tabs and editor results.
- JSON view and cell viewer.

## Out of scope

Editing (Phase 7), export to file (Phase 8), column-level filter builders, grouping, charts.

## Design

```rust
pub struct TablePageRequest {
    pub table: TableRef,
    pub filter: Option<String>,          // raw WHERE expression typed by the user
    pub sort: Vec<SortKey>,              // column + direction; empty = row key order
    pub limit: usize,                    // default 200
    pub offset: u64,
}

pub enum RowCount { Exact(u64), Estimate(u64), Unknown }
```

- `Dialect::select_page` builds the statement; column names in `sort` are validated against the table's columns and quoted.
- The filter is rejected if the splitter finds more than one statement in it.
- Estimated counts: `pg_class.reltuples` for PostgreSQL, `information_schema.tables.table_rows` for MySQL, none for SQLite (exact count on request only).

## Tasks

- [ ] 6.1 **Spike: grid.** Render 1000 × 50 with row and column virtualisation, keyboard cell navigation, column resize and a custom cell renderer. Measure scroll smoothness. Confirm or replace the choice in DECISIONS D-006.
- [x] 6.2 `Dialect::select_page` for the three dialects. Snapshot tests: no filter, filter, multi-column sort, offset, identifiers needing quotes, table without a row key.
- [x] 6.3 `fetch_table_page` and `count_rows` in the three drivers. Conformance group.
- [ ] 6.4 `table_page` and `table_count` commands on the metadata connection, cancellable.
- [ ] 6.5 Grid core: virtualised rows and columns, sticky header and row-number column, column resize and auto-size to content, cell and range selection, keyboard navigation, copy as TSV.
- [ ] 6.6 Cell rendering by `TypeCategory`: right-aligned numbers, distinct styling for `NULL`, booleans, truncated previews for long text, JSON and binary (`<binary 2.4 MB>`), arrays.
- [ ] 6.7 Table data tab: toolbar with filter input (with history of recent filters per table), refresh, page controls, page size, row count (estimate first, "count exactly" action); sort by clicking a header, shift-click to add a secondary sort.
- [ ] 6.8 Filter errors are shown inline under the filter box with the server message; the previous page stays visible.
- [ ] 6.9 JSON view: toggle that renders the current rows as an array of objects in a read-only CodeMirror JSON editor. Numbers that are safe stay numbers; integers and decimals beyond double precision are emitted as strings; binary as base64; a note says so.
- [ ] 6.10 Cell viewer panel, opened for the selected cell: JSON pretty-printed and highlighted (falls back to raw text if it does not parse), XML likewise, plain text with wrap toggle, binary as hex dump with size, and image preview for common image signatures. Copy button.
- [ ] 6.11 `cell_full_value` command to fetch a value that was truncated on the wire, addressed by row key; for tables without a row key the viewer shows the truncated value and says so.
- [ ] 6.12 Foreign key navigation: cells in foreign key columns get a link affordance; activating it opens the referenced table filtered to that key.
- [ ] 6.13 Apply the grid upgrades and the JSON view to editor results.
- [ ] 6.14 Column header shows name, type and key markers (primary key, foreign key).
- [ ] 6.15 E2E tests: open table, sort, filter, page, open a JSON cell in the viewer, toggle JSON view.

## Acceptance criteria

- A table with 10 million rows opens in the time of one indexed page query; the grid shows an estimated count immediately.
- Sorting and filtering run on the server and reset to the first page.
- Scrolling a 1000 × 50 page stays smooth on the development machine.
- `NULL`, empty string and the text `NULL` are visually distinguishable.
- A 5 MB JSON cell does not slow the grid; the viewer loads it on demand and pretty-prints it.
- Invalid JSON in a `text` column shown through the viewer does not throw.
- Following a foreign key lands on the referenced row.
- Cell content containing HTML or script is displayed as text.
- The JSON view of a row with a `bigint` above 2^53 shows the exact digits.

## Manual smoke

1. Browse the largest table you have access to.
2. Inspect real JSON and XML columns.
3. Filter with a deliberately wrong expression and recover.
