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
