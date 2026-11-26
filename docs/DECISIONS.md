# Decision log

Append-only. To change a decision, add a new entry that supersedes the old one.

Format: context, decision, consequences. Keep entries short.

---

## D-001 Tauri 2 with a web frontend (2026-11-14)

**Context.** The three hardest features are a SQL editor with autocomplete, an editable virtualised grid, and JSON/XML viewers. Pure-Rust GUI toolkits would need most of these built by hand.
**Decision.** Tauri 2 shell, React + TypeScript frontend, all database logic in Rust.
**Consequences.** Two languages and an IPC boundary. Editor and grid come from mature web libraries. The Rust core stays UI-agnostic, so a different frontend remains possible.

## D-002 Native drivers behind our own trait, not sqlx (2026-11-14)

**Context.** A generic client must decode columns whose types are unknown at compile time, cancel queries, and introspect catalogs. `sqlx` is designed around statically known queries and types.
**Decision.** `tokio-postgres`, `mysql_async` and `rusqlite`, each wrapped in a crate implementing `dbc-core` traits.
**Consequences.** Three APIs to learn; full control over decoding and protocol features. A shared conformance suite keeps drivers consistent.

## D-003 Database values cross IPC as tagged strings (2026-11-14)

**Context.** JavaScript numbers lose precision above 2^53 and cannot represent `numeric`. JSON has no way to distinguish `NULL`, empty string and bytes.
**Decision.** `Value` serialises as `{ t, v }` with string payloads.
**Consequences.** Slightly larger payloads; the frontend formats values instead of doing arithmetic on them. No precision bugs.

## D-004 SSH tunnel as a loopback listener (2026-11-14)

**Context.** Drivers differ in whether they accept a custom stream.
**Decision.** `dbc-tunnel` forwards a `127.0.0.1` port over `russh`; drivers connect to it as an ordinary TCP endpoint.
**Consequences.** Works with every driver unchanged. The port is reachable by other local processes while open, as with `ssh -L`. TLS must verify against the real hostname, so configs carry dial address and server name separately.

## D-005 Edits are staged and applied in one transaction (2026-11-14)

**Context.** Inline edits that write immediately are the easiest way to damage data.
**Decision.** Edits accumulate as a `ChangeSet`; the user reviews generated SQL; apply is one transaction that requires each `UPDATE`/`DELETE` to affect exactly one row. Tables without a row key are read-only. Ad-hoc query results are read-only in v1.
**Consequences.** One more click per edit session. No partial applies.

## D-006 Grid: TanStack Table + TanStack Virtual (2026-11-14)

**Context.** The grid needs row and column virtualisation, keyboard navigation, inline editing and custom cell rendering.
**Decision.** Headless TanStack Table with TanStack Virtual, with our own cell rendering and editing.
**Consequences.** More UI code than a batteries-included grid, full control over behaviour and styling, permissive licence. **Revisit at the start of Phase 6** with a one-day spike on a 1000 × 50 result; if scrolling or editing is not good enough, evaluate a canvas-based grid and record the outcome here.
**Outcome of the Phase 6 spike (2026-11-14).** Kept, without TanStack Table. The grid (`features/grid/DataGrid.tsx`) virtualises rows and columns with TanStack Virtual and renders cells itself; TanStack Table added nothing once sorting moved to the server and was removed. Measured in Chromium (`e2e/grid-performance.spec.ts`) on a 1000 × 50 result of mixed types, scrolling to a new position every frame across the whole grid: p50 16.6 ms, p95 22.8 ms per frame, about 400 cells in the DOM. Keyboard navigation, range selection, column resizing and custom cell rendering all fit the headless approach; a canvas grid is not needed. Inline editing (Phase 7) is still to be proven.

## D-007 One database connection per editor tab (2026-11-14)

**Context.** Session state (`BEGIN`, `SET`, temp tables) and cancellation are ambiguous on shared or pooled connections.
**Decision.** Each editor tab owns a connection; the schema browser and grids share a separate metadata connection.
**Consequences.** More connections per session (bounded by open tabs). Simple mental model, matching `psql`-style sessions.

## D-008 Ad-hoc statements are split client-side and run one at a time (2026-11-14)

**Context.** "Run statement under cursor" and per-statement results need statement boundaries; drivers differ in multi-statement support.
**Decision.** A lexer-level splitter in `dbc-core`, dialect-aware for quoting and comments. MySQL `DELIMITER` is unsupported in v1.
**Consequences.** Stored-routine bodies in MySQL scripts will not split correctly until `DELIMITER` is handled. PostgreSQL dollar-quoted bodies work.

## D-009 Local state in SQLite plus OS keychain (2026-11-14)

**Context.** Profiles, history and saved queries need durable, queryable storage; secrets must not be on disk in plain text.
**Decision.** One SQLite file in the app data directory, forward-only migrations with `PRAGMA user_version`; secrets in the keychain via `keyring`.
**Consequences.** Profiles are not portable between machines without re-entering secrets. Import/export of profiles is deferred.

## D-010 Rust-to-TypeScript types generated with ts-rs (2026-11-14)

**Context.** Hand-maintained duplicate types drift.
**Decision.** IPC types derive `TS`; output is committed and checked in CI. Command wrappers are hand-written and thin.
**Consequences.** One generation step. **Verify in Phase 1** that `ts-rs` handles the tagged `Value` enum with custom serialisation; if not, hand-write that one type with a round-trip test.

## D-011 E2E tests run the frontend against mocked IPC (2026-11-14)

**Context.** Tauri's WebDriver tooling does not support macOS.
**Decision.** Playwright drives the frontend in a browser with mocked commands; Rust command tests cover the backend; a manual smoke list covers the join.
**Consequences.** No automated test crosses the real IPC boundary. Generated types and shared fixtures reduce the gap.

## D-012 Crate prefix `dbc-` (2026-11-14)

**Context.** A crate named `core` collides with Rust's built-in `core`.
**Decision.** Workspace crates are `dbc-core`, `dbc-driver-*`, `dbc-tunnel`, `dbc-store`.
**Consequences.** None beyond naming. The product name is still open.

## D-013 `async-trait` for driver traits (2026-11-14)

**Context.** `Box<dyn Connection>` / `Arc<dyn Connection>` requires dyn-compatible traits. Native `async fn` in traits is not dyn-compatible; making it so means hand-written boxed-future signatures in every driver.
**Decision.** `Driver`, `Connection` and `CancelHandle` use `#[async_trait]`.
**Consequences.** One heap allocation per call, which is negligible next to a database round trip. Signatures stay readable. Revisit if the language gains dyn-compatible async traits.

## D-014 Product name: Dunlin (2026-11-14)

**Context.** D-012 left the product name open. It must be fixed before the Tauri bundle identifier (which decides the app data directory) and the keychain service name are set, because renaming either later orphans stored data.
**Decision.** The product is **Dunlin**. Bundle identifier `dev.dunlin.desktop`, keychain service `dunlin`, app data directory as derived by Tauri from the identifier. Crate prefix stays `dbc-`.
**Consequences.** Supersedes the open question in D-012. Checked free on crates.io only; trademark and domain are unchecked.

## D-015 `Value` wire type is a separate, generated enum (2026-11-14)

**Context.** D-010 asked to verify that `ts-rs` handles the tagged `Value` type with custom serialisation. Display truncation also needs fields (`len`, `truncated`) that the domain `Value` does not have.
**Decision.** `dbc_core::WireValue` is an internally tagged serde enum (`#[serde(tag = "t")]`) deriving `TS` and exported as `Value`. The domain `Value` converts into it with `into_wire(limit)`; `Value`'s own `Serialize`/`Deserialize` go through `WireValue`. Verified in Phase 1: the generated TypeScript is a discriminated union matching DRIVER_API.md, so nothing is hand-written. Generated files are checked by `cargo test -p dunlin bindings` and rewritten with `DUNLIN_UPDATE_BINDINGS=1`. 64-bit integers outside database values are generated as `number`, which is what `serde_json` actually sends.
**Consequences.** Supersedes the verification note in D-010. Converting rows to the wire form allocates one string per number, which is the cost D-003 already accepted.

## D-016 PostgreSQL values are decoded from the binary protocol (2026-11-14)

**Context.** Phase 2 spike 2.1: prove the `RawValue` plan from DRIVER_API.md or fall back to the simple (text) protocol.
**Decision.** Keep the binary plan. Statements are prepared (which also gives typed columns) and read with `query_raw`. A `RawCell` `FromSql` accepts every type, and our own decoder turns the bytes into `Value` by type name and `Kind`. Verified against PostgreSQL 18 for `numeric` (30+ digits, NaN), `json`/`jsonb`, `xml`, `date`/`time`/`timetz`/`timestamp`/`timestamptz`/`interval` (BC dates, infinity, PostgreSQL's interval style compared with the server's own text output), `uuid`, `bytea`, `inet`/`cidr`, `macaddr`, `varbit`, `money`, an enum, a domain, arrays (enum arrays, NULLs, multi-dimensional, empty), `int4range`/`daterange`, a multirange, a composite type, an anonymous record, and the `hstore` and `citext` extensions. The simple protocol was rejected because `tokio-postgres` does not expose column types for it.
**Consequences.** Types without a `Value` variant that we can render (ranges, composites, network types, `hstore`) are `Value::Unknown` with PostgreSQL-style text; types no one has written a decoder for (e.g. `tsvector`, PostGIS) are `Value::Unknown` with hex. Malformed payloads never panic; they fall back to hex. `timestamptz` is shown in UTC (`+00:00`), not in the session time zone. Each new extension type needs a decoder, or it shows as hex.

## D-017 Stop at the row limit by cancelling the statement (2026-11-14)

**Context.** Phase 2 spike 2.2. When the row limit is reached, unread rows must not cost the user anything. Measured on 3-5 million row results: abandoning the stream made the next query wait 3.1 s (PostgreSQL 18) and 3.8 s (MySQL 8.4), because the client drains the rest of the result; cancelling first made it about 5 ms on both. MariaDB 11.4 behaves like MySQL.
**Decision.** When a driver has read `row_limit` rows and another row is available, it marks the result `truncated`, cancels the statement (PostgreSQL cancel request; `KILL QUERY` on MySQL/MariaDB) and **waits for the cancel to be acknowledged** before draining the remainder of the response and swallowing the resulting "cancelled" error. Waiting first means a late cancel can never hit the next statement on the connection.
**Amended (Phase 4).** Waiting for the acknowledgement is not enough on PostgreSQL: the server can deliver the cancel after the statement has finished, and it then interrupts the next statement (reproduced by the repeated truncations in the conformance row-limit group). The PostgreSQL driver now first keeps reading for up to 10 000 rows or 50 ms, and needs no cancel at all when the statement ends within that; otherwise it cancels as above, and if the statement still ends normally it runs `SELECT pg_sleep(0.1)` to take a late cancel. A truncated result of a statement that is just over the limit therefore costs up to 50 ms, and the rare late cancel 100 ms.
**Consequences.** One extra short-lived connection per truncated result. A statement that has side effects and is truncated (e.g. `INSERT ... RETURNING` with more rows than the limit) is cancelled and therefore rolled back; the UI reports it as truncated. The SQLite driver keeps dropping the statement, which costs nothing there.

## D-018 Drivers declare their TLS modes; MySQL has no `verify-ca` (2026-11-14)

**Context.** `mysql_async` 0.37 implements "skip domain validation" by searching the rustls error's display text for `NotValidForName`, which never appears in it, so a chain-only check always fails. It exposes no way to install our own certificate verifier.
**Decision.** `DriverInfo` lists the TLS modes a driver supports, and the connection form offers only those. The MySQL driver supports `disable`, `prefer`, `require` and `verify-full` (checked against the real server name via the hostname override, so tunnels work) and refuses `verify-ca` with a clear error.
**Consequences.** MySQL servers whose certificate names do not match any reachable name must use `require` (encrypted, unverified, with the form's warning) until the library is fixed or replaced. Revisit on each `mysql_async` upgrade.

## D-019 SSH with `russh` 0.65 (2026-11-14)

**Context.** Phase 3 spike 3.1: prove the tunnel design on the current `russh` against real OpenSSH servers.
**Decision.** Keep `russh` (`ring` backend, as for TLS). Verified against OpenSSH in containers: host key check in the handler, key/password/encrypted-key/agent authentication, `direct-tcpip` forwarding of several concurrent connections, a jump host as a second SSH session over a channel of the first (`connect_stream`), keepalive-based loss detection. Differences from the phase brief:
- Key handling is part of `russh` (`russh::keys`); there is no separate `russh-keys`.
- `check_server_key` receives `PublicKeyOrCertificate`; host certificates are refused (certificates are out of scope).
- `SshAuth::Agent` carries the agent socket path. The app fills it from `SSH_AUTH_SOCK`; the library never reads the environment, so tests can use a private agent (setting variables is `unsafe` in Rust 2024).
- Host keys are compared per algorithm, as OpenSSH does: a different key of the same algorithm is `Changed`, a key of another algorithm is `Unknown`. The user's `known_hosts` is read with `russh`'s parser (hashed entries and `[host]:port` work); `@revoked` and `@cert-authority` lines are not understood and match nothing.
- `HostKeyStore::check` is synchronous; the app's table is loaded into a `FingerprintList` before connecting.
**Consequences.** Opening a tunnel probes the forward with one short `direct-tcpip` channel, so an unreachable database host is reported as `ForwardRefused` (stage `forward`) up front; the database may log one incomplete connection per tunnel opened. Later forwarded connections that the server refuses are closed immediately; the database driver then reports a lost connection, and the session reports `tunnel_lost` if the SSH connection itself is gone.

## D-020 Introspection shape shared by every driver (2026-11-14)

**Context.** Phase 4. The three databases report keys, indexes and actions differently: PostgreSQL and SQLite create indexes for unique constraints, MySQL calls every key an index, SQLite has no constraint names, PostgreSQL 18 stores `NOT NULL` as constraints, and MySQL and MariaDB disagree on `RESTRICT` versus `NO ACTION` and on the spelling of defaults. DRIVER_API.md also put PostgreSQL's database name in `NamespaceRef::catalog`.
**Decision.** One normalised shape, listed under "Schema model" in DRIVER_API.md: constraint-backing indexes are reported only as keys; `NO ACTION` (and MySQL's `RESTRICT`) is `None`; generated expressions live in `Column::default`; `Namespace::system` lets the frontend hide catalogs without a driver option; `DbErrorCode::NotFound` is new. `NamespaceRef::catalog` is `None` on PostgreSQL, since v1 only browses the connected database and a three-part name is not valid in PostgreSQL SQL anyway. The conformance group asserts the shape that is common to all three; type names, defaults and expression text are covered by per-driver `insta` snapshots, one set per MySQL flavour. SQLite reads checks, generated expressions, index expressions and predicates from the stored `CREATE` text, best effort.
**Consequences.** `Dialect::qualified_name` gives `"schema"."table"` on PostgreSQL. PostgreSQL DDL is a reconstruction: the round-trip test recreates every fixture table, an identity/domain/collation table, a partitioned table and a materialized view with the same description, but sequences behind `serial` defaults, grants, triggers, policies and partitions are not emitted. MySQL `CHECK` text is shown as each server stores it.

## D-021 Editor tabs in localStorage; dangers reported by `sql_split` (2026-11-14)

**Context.** Phase 5. Unsaved editor content must survive a restart, and a script must list every dangerous statement once before it starts, while the danger check stays in the backend (SECURITY.md).
**Decision.** Each connection's editor tabs (title, text, row limit, in tab-strip order) are saved in the webview's `localStorage` under `dunlin.tabs.<connection id>`, debounced, and restored when that connection opens. They are not in the store database: they are a UI convenience, not data the backend needs, and they hold only what the user typed. `sql_split` returns each statement's `danger` (from the same classifier `query_execute` uses), so the frontend asks once for the whole script and then sends `confirmed: true` with exactly those statements. `query_execute` still refuses an unconfirmed dangerous statement, and if it ever finds one the split did not, the frontend asks again and resumes from it. `Connection::default_namespace` gives autocomplete its default schema without the frontend knowing driver rules.
**Consequences.** Deleting a connection leaves its saved tabs in `localStorage` until overwritten; the text is never sent anywhere. Saved tabs do not follow the store file if the user moves it to another machine.

## D-022 Table pages are built once, in `dbc-core`, and run through `query_params` (2026-11-14)

**Context.** Phase 6. DRIVER_API.md planned `fetch_table_page` and `count_rows` on every connection and `select_page` on every dialect. The statements are the same for all three databases apart from quoting and placeholders, and key filters (foreign key navigation, full cell values) need bound parameters, which no driver had.
**Decision.** `dbc_core::data` builds the page, count and cell statements from the dialect and the table's description, and runs them through a new `Connection::query_params`. Drivers add only `Dialect::placeholder`, `query_params` and `estimate_rows` (`pg_class.reltuples`, `information_schema.tables.table_rows`, none for SQLite). PostgreSQL binds every parameter as text and casts it in the placeholder (`$1::text::integer`); MySQL binds native values over the binary protocol, which its decoder now also reads; SQLite binds its own value types. The conformance type table runs through both execution paths.
**Consequences.** A new driver gets table browsing by implementing three small methods. Table data views get their own connection, like editor tabs (D-007), rather than the metadata connection the phase brief named: that is what makes a page cancellable without cancelling the tree's work, and what keeps a slow page from blocking the tree on MySQL and SQLite, whose connections serialise. Filtered counts are exact-only (no estimate). Arrays cannot yet be bound as parameters.

## D-023 Edits are applied by one function in `dbc-core`; previews render the same statements (2026-11-14)

**Context.** Phase 7. DRIVER_API.md planned `apply_changes` on every connection. With `query_params` (D-022) the transaction, the ordering and the one-row check are the same everywhere; only quoting, placeholders and literals differ.
**Decision.** `dbc_core::changes::apply` runs a change set on any connection: `BEGIN` (through `execute`, since MySQL does not prepare every statement), deletes, updates and inserts through `query_params`, a check that each `UPDATE`/`DELETE` affected exactly one row, then `COMMIT`, or `ROLLBACK` with the index of the failing operation. Each statement is planned once as SQL text and typed values; the executed and the preview forms are both rendered from that plan. `Dialect::render_literal` gets a test of its own on every database: a literal and the bound parameter must store the same value, for every write type and for generated strings full of quotes, backslashes and non-ASCII. That test found that an unquoted decimal literal becomes a float on SQLite, so SQLite quotes decimals; MySQL writes strings with backslashes or control characters as `_utf8mb4 X'..'`, which mean the same in every `sql_mode`.
**Consequences.** Drivers need no edit code. A connection must not already be inside a transaction when applying, which holds for table views (their own connection, D-022).

## D-024 Exports stream through `Connection::stream_rows` (2026-11-14)

**Context.** Phase 8 exports whole tables (5 million rows in the acceptance criteria) with flat memory and cancellation. Paging with `LIMIT/OFFSET` would make the export quadratic, and holding results in memory is what the row limit exists to avoid.
**Decision.** `Connection::stream_rows` runs one statement and sends `StreamEvent::Columns`, then batches of 1000 rows, over a bounded Tokio channel (4 batches in flight). When the receiver goes away the driver stops reading and cancels the statement (D-017), so a failing writer or a cancelled export never drains the result. `dbc_core::export::ExportWriter` writes CSV (RFC 4180; empty text and text equal to the chosen `NULL` text are quoted so importers tell them from `NULL`; binary as `\x` hex), JSON (the JSON view's number rules; valid JSON documents embedded verbatim) and batched `INSERT` (`render_literal`, generated and identity-always columns left out). Files are written as `<name>.partial` and renamed when complete. `dbc-core` now depends on Tokio for the channel type only.
**Consequences.** Tests re-import the app's CSV with `COPY` (PostgreSQL), `LOAD DATA` (MySQL, MariaDB; `NULL` as the word `NULL`, `ESCAPED BY ''`, binary through `UNHEX`) and the `sqlite3` shell, which cannot represent `NULL` in CSV, and run exported `INSERT`s on empty copies. Memory is bounded by design (four batches); it is not measured by a test.

## D-025 Settings are one JSON document in the store; the light theme mirrors the palette (2026-11-14)

**Context.** Phase 8 task 8.8. The settings must survive upgrades, and the UI was written dark-only with Tailwind's `zinc`, `sky`, `rose`, `amber` and `emerald` classes.
**Decision.** Settings are one JSON document in a `settings` table (migration 7). Every field has a default, so a document from an older version still loads. `Settings::validate` refuses values out of range instead of clamping them. The backend caches the settings in `AppState`, since the history cap is read on every statement; a cap of 0 records nothing. The light theme sets `<html data-theme="light">`, and `theme-light.css` redefines Tailwind's colour variables with each scale mirrored: zinc 50↔950 through 400↔600; the accents 50–300↔950–700, with 400 taking 600's value. Components keep a single set of classes. CodeMirror has its own light theme and takes the editor font from the settings through a compartment. The "always preview" policy and a profile's `alwaysPreview` both force the preview.
**Consequences.** New UI is styled once, for dark, and gets the light theme for free. A colour outside the mirrored scales (`white`, `black`) is the same in both themes. Trusted SSH hosts stay in their own table and are opened from the settings view.

## D-026 Restoring the window: only the active connection reconnects at start (2026-11-15)

**Context.** Task 8.12: window state, sidebar width, open sessions and tabs come back on start, and sessions reconnect lazily.
**Decision.** Tauri's window-state plugin keeps the size, position, maximised and full-screen state. The sidebar width and the open connections, active first, are kept in `localStorage` (`dunlin.layout`), as editor tabs already are (D-021). At start only the previously active connection reconnects. The others are listed as "open last time" and reconnect, with their tabs, when the user opens them. A connection that fails to reconnect (server down, host key changed, missing password) shows its error the usual way and is not retried.
**Consequences.** Start-up opens at most one database connection (and SSH tunnel), so it does not wait on servers the user may not need. A `reconnectOnStart` setting (default on, D-038) turns the start-up reconnect off. Table data and details views are not restored: they are cheap to reopen, and restoring them would query every table at start.

## D-027 Editing query results: plain single-table selects only, applied on the tab's connection (2026-11-15)

**Context.** Deferred feature 1: the rows of an ad-hoc query should be editable when they map unambiguously to one table's rows.
**Decision.** `dbc-core::sql::single_table_source` accepts only `SELECT <*|t.*|columns> FROM [schema.]table [alias] [WHERE …] [ORDER BY …] [LIMIT/OFFSET/FETCH …]`, refusing joins, comma joins, grouping, `DISTINCT`, set operations, expressions, aliases, `INTO` and locking clauses. Unquoted names fold as the dialect stores them (`Dialect::fold_identifier`). An unqualified table is resolved on the tab's own connection, which may have run `USE` or `SET search_path`. The frontend then requires every result column to be one of the table's own, once, including the row key, and reuses the table editor (staging, preview, one transaction) with the tab as the view's connection. Only one-statement runs are editable; re-running or switching results with pending edits asks first. After an apply the statement runs again.
**Consequences.** Anything the lexer check cannot prove stays read-only, which errs towards safety: a view that looks like a table is still refused by the backend's edit rules, and a table the session cannot describe stays read-only.

## D-028 Manual transactions are per editor tab and begin lazily (2026-11-15)

**Context.** Deferred feature 2: a manual commit mode with a visible open transaction and Commit/Rollback.
**Decision.** Each editor tab has its own connection (D-007), so the mode is per tab and held by the session manager. In manual mode the backend sends `BEGIN` before the first statement that is not itself transaction control, and the transaction stays open until Commit, Rollback, or a `COMMIT`/`ROLLBACK` the user runs. Losing or closing the tab's connection ends it. Leaving manual mode with a transaction open is refused rather than committing or rolling back implicitly. Result editing (D-027) is off in manual mode, and `changes_apply` refuses such a tab, because the apply commits its own transaction. Closing the tab or quitting with an open transaction asks first. The mode is not persisted: a restored tab starts in auto-commit.
**Consequences.** The open flag is the backend's view: MySQL ends a transaction on DDL by itself, after which the flag stays set until Commit (a harmless no-op there). PostgreSQL aborts the transaction after an error, and statements fail until Rollback; the server's message says so.

## D-029 Visual EXPLAIN reads each server's own plan format, never ANALYZE (2026-11-15)

**Context.** Deferred feature 3: show a statement's plan as a tree.
**Decision.** `Connection::explain` returns a `dbc_core::plan::PlanNode` tree (operation, relation, details, estimated rows, total cost). PostgreSQL reads `EXPLAIN (FORMAT JSON)`; MySQL reads `EXPLAIN FORMAT=TREE` and falls back to the classic tabular `EXPLAIN` (one step per table) where the server refuses the format (MariaDB, MySQL before 8.0.16); SQLite builds the tree from `EXPLAIN QUERY PLAN`'s parent ids. Only plain `EXPLAIN` is offered, so the statement never runs, and it is allowed on read-only connections. The plan runs on the tab's connection, so it sees the tab's `search_path`, `USE` and open transaction.
**Consequences.** Costs are the server's estimates in its own units and are not comparable across servers; MariaDB and SQLite plans carry no cost. `EXPLAIN ANALYZE` stays available by typing it, with the usual safeguards.

## D-030 `~/.ssh/config` fills the SSH form; it is not linked (2026-11-15)

**Context.** Deferred feature 4: reuse the host aliases people already keep in `~/.ssh/config`.
**Decision.** `dbc-tunnel` parses the file as OpenSSH resolves it (first value wins, `Host` patterns with `*`, `?` and `!`, global settings first) for `HostName`, `User`, `Port`, `IdentityFile` and the first `ProxyJump` hop. The connection form offers the aliases and copies the values into the profile; a `ProxyJump` naming another alias becomes the jump host. `Match` blocks are skipped and `Include` is not followed.
**Consequences.** A saved connection never changes behind the user's back when the file does, and works the same on a machine without it. Settings the tunnel does not support (`ProxyCommand`, `LocalForward`, certificates) are ignored.

## D-031 SQLite tables without a key are edited by `rowid` (2026-11-15)

**Context.** Deferred feature 9: a SQLite table with no primary key or unique not-null key was read-only, although every rowid table has a stable address.
**Decision.** `TableInfo::row_id` names an implicit addressing column that is not among `columns`. The SQLite driver sets it for rowid tables (not `WITHOUT ROWID`, not views) to the first of `rowid`, `_rowid_`, `oid` that no real column shadows. `choose_row_key` falls back to it after real keys. Table pages then select it first (`SELECT "rowid", * ...`) and order by it, and edits address rows with `WHERE "rowid" = ?`. It is an `INTEGER` that cannot be written. The shared conformance suite ignores the implicit key, and SQLite's own tests cover it.
**Consequences.** Such tables show a leading `rowid` column marked as the key. A `VACUUM` may renumber rowids of tables without an `INTEGER PRIMARY KEY`, so a page read before one and edited after can address the wrong rows; this matches what other SQLite tools do and is rare in practice. Other drivers leave `row_id` empty (PostgreSQL's `ctid` changes on every update).

## D-032 Redis is a key-value driver behind the same traits (2026-11-15)

**Context.** Redis (and Valkey) connections were requested. Redis has no SQL, tables or schemas, so it cannot be a fourth SQL driver.
**Decision.** `dbc-driver-redis` implements `Driver`/`Connection` with `redis-rs` 1.8 over a socket the driver opens itself, so TLS verifies the real server name through an SSH tunnel (D-004). The rustls setup is copied from the PostgreSQL driver, because drivers depend only on `dbc-core`. `execute` runs one command with redis-cli quoting and shows the reply as a grid: scalars as one cell, arrays and sets one row per element, RESP3 maps one row per entry, nested replies as JSON. RESP3 is tried first, then RESP2. `Dialect` gains `split` and `classify` with the SQL behaviour as default. Redis splits one command per line (`#` and `//` comments) and classifies by command name: listed read commands are reads, `SELECT`/`AUTH` are session, `MULTI`/`EXEC` are transaction, anything else including unknown commands is a write. `FLUSHALL`, `FLUSHDB`, `SWAPDB`, `SHUTDOWN`, `DEBUG`, `KEYS` and script or function flushes ask first. `Capabilities::key_value` tells the frontend to drop the schema tree, SQL autocomplete, Explain and manual transactions; the editor gets a small command language instead. A Redis command cannot be stopped for one client, so cancel closes the connection, `Connection::is_broken` tells the session, and the next command reconnects. The profile's database field is the database number. Subscriptions and `MONITOR` are refused, and Cluster and Sentinel are not supported.
**Consequences.** Connections, keychain, TLS, SSH, sessions, tabs, history and the result grid work unchanged. The key browser lists keys with `SCAN` steps, never `KEYS`, and batches `TYPE` in one pipeline. The key view shows a key's elements in the result grid. Each key is addressed by its bytes in base64, so keys that are not valid UTF-8 still work. Edits are typed (`KeyEdit`), never free text. The backend plans each edit into exact commands, and the frontend shows them before Apply runs them. Values are written as text, so binary values and stream or JSON values stay console-only. `SET` keeps the TTL, rename never overwrites an existing key (`RENAMENX`), and a "new key" form refuses a name that already exists.


## D-033 Table pages seek by keyset when the order allows it (2026-11-15)

**Context.** Table pages used `LIMIT n OFFSET m`. The database reads and discards `m` rows for every page, so pages deep into a large table slow down. Rows inserted or deleted between pages also shift what each page shows.
**Decision.** A page's order is always the sort followed by the row key columns the sort leaves out, so it is a total order. When the table has a row key and no order column is nullable, `table_page` returns a `cursor`: the order values of the page's last row, sent whole and never truncated. The next request sends that cursor back as `after`. The page then starts with `c1 >= ? AND (c1 > ? OR (c1 = ? AND c2 > ?) ...)`, using `<` for descending columns. It has no `OFFSET`. The leading bound lets every database range-scan an index on the first column. Expanded comparisons are used instead of row values (`(a, b) > (?, ?)`) because they handle mixed directions and behave the same on all three databases. Nullable order columns fall back to `OFFSET`, because databases place `NULL` differently. Tables without a row key also use `OFFSET`. The frontend keeps the cursor each visited page started from, so Previous seeks too. `offset` still numbers the rows shown. A new filter, sort or page size starts again from the first page.
**Consequences.** Next and Previous cost the same on any page of a large table when the sort is on the key or on non-null columns. Row numbers are counted from the first page, so they can drift when other sessions insert or delete rows while you page. Adding the row key as a tiebreaker also makes `OFFSET` pages and exports in sorted order deterministic.

## D-034 CSV import is typed, batched and all-or-nothing (2026-11-15)

**Context.** Rows could be exported to CSV but not brought back in. The CSV export quotes empty text and text equal to the `NULL` text, so readers can tell it from `NULL` (D-024). Common CSV readers drop whether a field was quoted, which loses that difference.
**Decision.** `dbc_core::import` has its own small RFC 4180 reader that keeps, for each field, whether it was quoted. It also drops a UTF-8 byte order mark, skips blank lines, reports malformed quotes, ragged rows and non-UTF-8 text with their line, and accepts line breaks inside quoted fields. An unquoted field equal to the `NULL` text is `NULL`; a quoted field is always text. Fields become values of the column's category. Integers, numbers, decimals and booleans (`true`/`false`, `t`/`f`, `yes`/`no`, `1`/`0`) are checked, so a bad value names its line and column. Binary accepts the export's `\x` hex. Other types go through as text, and the database checks them. The file is read on a blocking thread and handed over in batches of 1000 records over a bounded channel, so a large file is never held in memory. Rows go in as multi-row `INSERT`s with bound values: up to 500 rows per statement, and fewer for wide tables so a statement stays under 30,000 parameters (SQLite's limit is 32,766). Everything runs in one transaction on the import's own connection. `query_cancel` stops it, and any error rolls the whole import back. The frontend maps CSV fields to columns by header name, ignoring case, spaces, `_` and `-`, or by position when there is no header. Unmapped columns take their default, and generated columns cannot be mapped. Read-only connections offer no import.
**Consequences.** A CSV written by the export imports back with `NULL` and empty text kept apart and binary values intact. A refused `INSERT` names the range of lines its rows came from, not the single bad row. A row-by-row retry to find it was left out, because PostgreSQL aborts the transaction on the first error. Very large imports hold one long transaction. `COPY` and `LOAD DATA` would be faster, but they need server-side file access or driver-specific streaming; they are left for later.

## D-035 Linux: deb, rpm and AppImage; passwords in the Secret Service (2026-11-15)

**Context.** Linux packages were requested. The keychain store was macOS-only, and on any other platform `KeychainSecretStore::open` failed, so the app could not even start.
**Decision.** On Linux, secrets go to the freedesktop Secret Service (GNOME Keyring, KWallet) through `zbus-secret-service-keyring-store`. It is pure Rust over zbus, so the packages need neither libdbus nor OpenSSL. The keyring service name stays `dunlin`. The store connects on first use instead of at start, and a failed connection is retried on the next use. So Dunlin starts without a keyring, connections without passwords still work, and saving a password says the keychain is not available and to run GNOME Keyring or KWallet. On macOS the keychain is still opened at start. `tauri.linux.conf.json` makes a deb, an rpm and an AppImage, built on Ubuntu 22.04 so the packages run on glibc 2.35 or newer. The release workflow builds x86_64 and arm64 on GitHub's native runners, checks the package contents, installs the deb and starts it under Xvfb with no keyring. A separate job gathers the macOS and Linux packages and a `SHA256SUMS` into one draft release. CI runs the real keychain round trip against GNOME Keyring under `dbus-run-session`.
**Consequences.** One release has every package. The Linux packages are unsigned, and no apt or dnf repository serves them. The AppImage leaves out the GStreamer media framework, which the app does not use. Flatpak and Snap are not built: their sandboxes need portal-based access to files and the Secret Service, and nobody has tested that. Windows still has no keychain store.

## D-036 Signed in-app updates from the GitHub release (2026-11-15)

**Context.** Users asked for "check for updates" and an automatic check at startup. The releases are draft GitHub releases with a dmg, deb, rpm and AppImage. The choice was between notify-only (link to the release page) and installing in the app; installing was chosen.
**Decision.** `tauri-plugin-updater` (2.14), driven from Rust: `update_check` asks `https://github.com/akyrey/dunlin/releases/latest/download/latest.json` and remembers the update it found; `update_install` downloads it, verifies its minisign signature against the public key in `tauri.conf.json`, installs it and restarts. The webview gets no updater permission and makes no request. The plugin installs the bundle the app came from: the macOS `.app`, the AppImage in place, and deb and rpm through `pkexec` (graphical sudo as fallback). The bundler signs those bundles only with `createUpdaterArtifacts`, which `tauri.updater.conf.json` turns on for release builds, so local and CI builds need no key. `scripts/updater-manifest.mjs` writes `latest.json` with `{os}-{arch}-{bundle}` keys (and `linux-{arch}` for the AppImage); one universal `.app.tar.gz` serves both Mac architectures. A tag build fails without `TAURI_SIGNING_PRIVATE_KEY`. Settings has `checkForUpdates` (default on); with it off nothing is requested at startup; "Check for updates" in the palette and "Check now" in Settings always work. A startup check that fails is logged only; a manual one shows the error. The prompt warns that unapplied edits and open transactions are lost by the restart.
**Consequences.** `latest/download` resolves only to a published release, so a draft is invisible to installed copies until it is published. Losing the private key means installed copies can never update again; it must be backed up. `requireSignedVersion` is on: the bundler writes the version into each signature's trusted comment (checked on a local build), so a forged manifest cannot pair a newer version number with an older, validly signed bundle. Signing a bundle by hand with `tauri signer sign` records no version, and such a bundle would be refused. Updates contact GitHub; no data is sent beyond the request itself. Windows has no build, so no Windows key.

## D-037 The macOS app is always signed; self-signed without a Developer ID (2026-11-16)

**Context.** Notarisation needs the paid Apple Developer Program, which the project does not have. Unsigned universal builds can show "Dunlin is damaged" and need `xattr -cr` to open. Passwords live in the macOS keychain, which grants an app access by its signature's designated requirement.
**Decision.** `release.yml` always signs the app, with the first available of: a Developer ID certificate (`APPLE_*` secrets, notarised); a self-signed code signing certificate (`SELF_SIGNED_CERTIFICATE`, `SELF_SIGNED_CERTIFICATE_PASSWORD`); ad hoc (`APPLE_SIGNING_IDENTITY=-`). Tauri's own certificate import only finds certificates with Apple-issued names, so the workflow imports the self-signed one into a keychain of its own and passes its SHA-1 hash as the identity. `codesign` accepts the untrusted certificate (checked locally: the app verifies with `--strict` and runs with the hardened runtime, as does an ad hoc build). Every build is checked with `codesign --verify`; `spctl` only when notarised.
**Consequences.** Without a Developer ID, Gatekeeper still blocks the first launch; users approve it once in System Settings → Privacy & Security. The updater downloads without the quarantine flag, so updates do not ask again. With the self-signed certificate the designated requirement (bundle id and certificate) is the same in every release, so the keychain keeps trusting the app across updates; ad hoc builds are asked again after each update, so ad hoc is the fallback only. The certificate must be backed up: a new one, or a later move to a Developer ID, makes each installed copy ask for keychain access once more. An ad hoc build leaves the DMG unsigned.

## D-038 Reconnecting the last active connection at start is a setting (2026-11-18)

**Context.** D-026 reconnects the previously active connection at start. A user found that surprising: a connection they had left open was live again after a restart, without being asked.
**Decision.** `Settings` has `reconnectOnStart` (default on, so D-026's behaviour is unchanged for existing users). With it off, every connection open when the app quit, the active one included, comes back as "open last time" and connects only when the user opens it. The layout restore waits for the stored settings as well as the profiles, so a user who turned it off never sees a connection open.
**Consequences.** Start-up opens no database connection or SSH tunnel at all with the setting off. Editor tabs still come back with their connection when it is opened.

## D-039 Code shared with Whimbrel lives in a kit, then in the Roost repository (2026-11-18)

**Context.** Whimbrel, an HTTP client, reuses Dunlin's editor, grid, command palette, shortcuts, updater, keychain, logging and release workflow. Both apps stay in their own repositories; the shared code goes to a third, Roost (`akyrey/roost`), that both depend on by tag (Whimbrel's D-H09).
**Decision.** First, inside Dunlin, the shared code moves behind app-neutral interfaces: frontend packages under `apps/desktop/src/kit/<package>`, imported as `@roost/<package>` through a tsconfig and Vite alias; Rust crates `crates/roost-shell` and `crates/roost-keychain`; the reusable `.github/workflows/release-app.yml`. ESLint forbids kit code from importing the app and the app from importing kit files by path. Seams: the grid is generic over values and columns through a `CellAdapter`; editors take extra languages as a prop and the theme from a CodeMirror-free context (so CodeMirror still loads on first use); the palette takes generic items; the updater store and prompt take the app's name and IPC calls; the keychain takes a service name and accounts; logging takes a file prefix and tracing target. Dunlin keeps thin wrappers with its old signatures, so its call sites and tests are unchanged. The tab strip stays in Dunlin: Whimbrel's tabs differ.
**Consequences.** Moving the kit to Roost is a move, not a redesign: the `@roost/*` imports stay, the alias is replaced by git dependencies. Until then a kit change is reviewed with Dunlin's whole suite. Code only one app uses stays in that app.

## D-040 Dunlin uses Roost by tag (2026-11-18)

**Context.** D-039 extracted the shared kit inside Dunlin; it now lives in the Roost repository (`akyrey/roost`).
**Decision.** Dunlin depends on Roost `v0.1.0`: Cargo git dependencies for `roost-shell` and `roost-keychain`, pnpm git dependencies with a sub-path for `@roost/*`, and `release.yml` calls Roost's `release-app.yml` at the same tag. The packages are TypeScript source: Vitest inlines them, Vite dedupes their peers (React, Zustand, CodeMirror, TanStack Virtual), Tailwind reads their classes through `@source`, and `tsconfig.json` pins the React types. `scripts/link-roost.sh` points both manifests at a local checkout for work across the two repositories; CI fails if a linked manifest is committed.
**Consequences.** A kit change reaches Dunlin only when Dunlin bumps the tag, and the bump's CI is the check. Cargo fetches a git dependency even when a `[patch]` replaces it, so linking rewrites the manifests rather than patching them.

## D-041 The connection list shows each driver's logo (2026-11-19)

**Context.** Saved connections were told apart only by name and the user's colour dot, so a PostgreSQL connection looked like a Redis one. The frontend must not branch on `DriverKind` (DRIVER_API).
**Decision.** `DriverIcon` maps every `DriverKind` to a logo from `simple-icons` (CC0), drawn monochrome in the text colour so it does not compete with the connection's colour. The map is a `Record<DriverKind, …>`, so a new driver fails to compile until it has an icon. Redis and Valkey share the Redis logo; simple-icons has no Valkey logo.
**Consequences.** This is the one place the frontend reads `DriverKind`, and only for presentation: features still come from `Capabilities`.

## D-042 Schema edits are explicit changes planned by each driver (2026-11-25)

**Context.** Phase 9: change a table's structure from its detail tab. The three databases differ more here than anywhere else: PostgreSQL has transactional DDL; MySQL commits every DDL statement and needs the full column definition to change one attribute; SQLite can only add, rename and drop columns and needs a table rebuild for anything else. Statement generation cannot be one function over the dialect, as row edits are (D-023).
**Decision.** The frontend sends a list of explicit `SchemaChange`s with the `TableInfo` they were made against (a diff of two descriptions cannot tell a rename from a drop and an add). Validation and the runner are shared in `dbc_core::schema_edit`; `Connection::plan_schema_edit` is per driver and may read the database but never changes it. The plan is text: DDL takes no parameters, so identifiers are quoted, comments are string literals and the rest is the user's SQL. PostgreSQL and SQLite run a plan in one transaction. MySQL puts every change it can into one `ALTER TABLE`, which is atomic on MySQL 8 and on MariaDB's copy algorithm; when a plan has more than one statement it runs them in order, stops at the first failure, reports which ones stayed applied, and the preview warns before applying. Because `CHANGE COLUMN` restates the whole column and `TableInfo` lacks `AUTO_INCREMENT`, `ON UPDATE`, character set and collation (and MySQL and MariaDB spell defaults differently), MySQL alters a column by editing its line from `SHOW CREATE TABLE`, replacing only the changed clauses. SQLite rebuilds the table, following the manual's procedure with foreign keys off, when native `ALTER TABLE` is not enough, and builds the new `CREATE TABLE` by editing the stored text piece by piece, so collations, `AUTOINCREMENT`, table options and generated columns survive. The table is described again before applying, and a changed description refuses the apply.
**Consequences.** A new driver gets schema editing by implementing one planner. A SQLite rebuild rewrites the whole table, so it is marked destructive and needs confirmation, like any drop; views and triggers on other tables that name a renamed or dropped column are reported, not rewritten. Column order is not editable: new columns are appended.

## D-043 Measured plans run the statement; hints suggest an index only for one-column filters (2026-11-25)

**Context.** Phase 10. D-029 offered only plain `EXPLAIN`, so a plan showed estimates and never ran anything. Seeing real times needs `EXPLAIN ANALYZE`, which runs the statement. Index advice is easy to get wrong for anything but the simplest filters.
**Decision.** Supersedes D-029's "never ANALYZE". An "Explain analyze" action runs the statement under the same safeguards as running it: reads only on read-only sessions, confirmation for dangerous statements, and a write only where it can be rolled back (PostgreSQL, SQLite), always rolled back, and never in a manual transaction. MySQL and MariaDB get it for reads only. Plan hints are computed in `dbc-core` from `PlanNode`, the same for every server. A full-scan hint suggests an index only when the filter is a conjunction of simple comparisons on one column of one table that no index starts with; the suggestion stages an `AddIndex` in the Phase 9 editor and runs nothing. Repeated timing ("run N times") is left out.
**Consequences.** Explain analyze takes as long as the statement and can be cancelled like a run. Most plans get hints without a suggestion; that is deliberate. Plain Explain is unchanged and still never runs the statement.


## D-044 JSON cells are edited pretty-printed and stored minified (2026-11-26)

**Context.** A JSON value edited in the cell viewer opened as stored, usually one long line, and was saved exactly as typed, so the indentation added while editing ended up in the database.
**Decision.** The editor opens a JSON value pretty-printed (two spaces) and "Set value" stores it minified. Both work on the text's tokens, not through `JSON.parse`/`JSON.stringify`, which only validates: numbers keep every digit, and strings, key order and duplicate keys stay as written. Values that do not parse, or were cut short, open as stored. Text columns that hold JSON are left alone, since their whitespace is the value. The cell viewer can be expanded to most of the result area; the setting holds while moving between cells.
**Consequences.** Saving a JSON cell rewrites its whitespace even when nothing else changed. That matters only for PostgreSQL `json` (not `jsonb`) and SQLite, which keep the text as written; MySQL and `jsonb` normalise it anyway.

## D-045 The editor formats SQL with `sql-formatter`, whitespace only (2026-11-26)

**Context.** Pasted or hand-written queries are often one long line, and there was no way to lay them out.
**Decision.** A "Format" button and ⇧⌘F format the selection, or the whole text when nothing is selected, with `sql-formatter` in the tab's dialect (PostgreSQL, MySQL, SQLite) and two-space indentation. Keyword and identifier case stay as written, so only whitespace changes. The edit is one undoable change. Scripts with a MySQL `DELIMITER` line are refused, since the formatter does not know about it and would break them; text that does not parse is left alone and the parse error is shown. Redis tabs have no Format button.
**Consequences.** A new dependency (about 3 MB unpacked, only the three dialects are imported). Formatting moves comments onto their own lines where the formatter puts them.
