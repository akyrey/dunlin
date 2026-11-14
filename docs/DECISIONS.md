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
**Consequences.** A new driver gets table browsing by implementing three small methods. Filtered counts are exact-only (no estimate). Arrays cannot yet be bound as parameters.
