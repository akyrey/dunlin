# Phase 2: PostgreSQL and MySQL

## Goal

The app connects to PostgreSQL and MySQL/MariaDB with passwords stored in the keychain and TLS, runs queries with correct value decoding, and can cancel them. The driver abstraction is proven against three real databases.

## Read first

[DRIVER_API.md](../DRIVER_API.md) (per-driver notes), [SECURITY.md](../SECURITY.md) (secrets, TLS), [TESTING.md](../TESTING.md) (conformance suite).

## In scope

- `dbc-driver-postgres` and `dbc-driver-mysql`: connect, ping, execute, cancel, close, read-only sessions, TLS.
- Keychain storage for secrets in `dbc-store`.
- Connection form for network databases, including TLS settings.
- `query_cancel` command and a cancel button.
- Read-only connection flag, enforced in the session and in the backend.
- Connection colour tag.

## Out of scope

Introspection (Phase 4), table paging (Phase 6), `apply_changes` (Phase 7), SSH (Phase 3).

## Tasks

- [ ] 2.1 **Spike: PostgreSQL decoding.** Prove the `RawValue` approach from DRIVER_API.md against `numeric`, `jsonb`, `json`, `xml`, `timestamptz`, `timestamp`, `date`, `time`, `interval`, `uuid`, `bytea`, `inet`, enum, domain, arrays (including of enums and with nulls), a range type and one extension type. Record the outcome in DECISIONS.md; if it fails, switch to the documented fallback and update DRIVER_API.md.
- [ ] 2.2 **Spike: early termination.** For both drivers, measure what happens when reading stops at the row limit on a multi-million-row result. Choose between dropping the stream and cancelling, and record it.
- [ ] 2.3 Fixtures `postgres.sql` and `mysql.sql`; `testcontainers` harness giving each test its own database.
- [ ] 2.4 `dbc-driver-postgres`: connect (password, TLS modes, connect timeout, application name), `ping`, `execute`, error mapping with SQLSTATE and position, `CancelHandle`, read-only session.
- [ ] 2.5 PostgreSQL type decoding to `Value` for everything covered in 2.1, with a per-type test table.
- [ ] 2.6 `dbc-driver-mysql`: connect (password, TLS modes), `ping`, `execute`, error mapping, `CancelHandle` via `KILL QUERY`, read-only session.
- [ ] 2.7 MySQL type decoding: integer widths signed and unsigned, `DECIMAL`, `FLOAT`/`DOUBLE`, `BIT`, `DATE`/`TIME`/`DATETIME`/`TIMESTAMP` including zero dates, `JSON`, `ENUM`/`SET`, `BLOB`/`BINARY` versus `TEXT`/`CHAR`, `YEAR`.
- [ ] 2.8 Conformance suite: add cancellation, read-only and error groups; run all three drivers. Run MySQL tests against both MySQL and MariaDB images.
- [ ] 2.9 `dbc-store`: keychain wrapper behind a trait (in-memory fake for tests), secret kinds, delete-on-connection-delete. Confirm `keyring` platform features against its docs.
- [ ] 2.10 `connection_save`/`connection_test` accept secrets; `ConnectionProfile` exposes only `hasDbPassword`-style flags. Test that no command response contains a secret.
- [ ] 2.11 Read-only enforcement in `query_execute` using `sql::classify`, in addition to the server-side setting.
- [ ] 2.12 `query_cancel` command; session manager tracks the running statement per tab.
- [ ] 2.13 Frontend: connection form for PostgreSQL and MySQL driven by `driver_list` option fields; TLS section with file pickers and a warning for non-verifying modes; colour tag and read-only toggle; cancel button with running state and elapsed timer.
- [ ] 2.14 Status bar shows connection name, colour and read-only badge.
- [ ] 2.15 Connection loss: kill the server mid-session in a test and assert `connection_lost`, then a successful retry after restart.

## Acceptance criteria

- All three drivers pass the conformance suite groups implemented so far.
- Every type in tasks 2.5 and 2.7 displays correctly in the grid, including a `numeric` with 30+ digits and a `BIGINT UNSIGNED` at its maximum.
- `SELECT pg_sleep(30)` and `SELECT SLEEP(30)` are cancelled within a second and the tab can run another query.
- A read-only connection rejects `INSERT`, `UPDATE`, `DELETE` and DDL on all three databases.
- With `verify-full`, a server presenting a certificate for the wrong host is refused.
- After saving a connection, the password is in the keychain and nowhere in the store file (asserted by a test that scans the file).
- Coverage at or above 80% for both new driver crates.

## Manual smoke

1. Connect to a local PostgreSQL and a local MySQL, run queries across several types.
2. Connect to a remote database that requires TLS.
3. Start a long query, cancel it, run another.
4. Restart the app and reopen a connection without re-entering the password.
