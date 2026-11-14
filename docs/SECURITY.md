# Security

A database client holds production credentials and can destroy data. These rules are requirements, not suggestions.

## Threat model

In scope:

- Secrets leaking to disk, logs, crash output or the webview.
- Man-in-the-middle on SSH or TLS connections.
- Accidental destructive statements against the wrong database.
- Malicious content in database values (a cell containing HTML or script) executing in the webview.
- SQL injection through identifiers or values in SQL that the app generates.

Out of scope: a compromised local user account or OS, and the SQL the user deliberately types.

## Secrets

- Stored only in the OS keychain through `keyring`, under service `dunlin`, account `<connection id>:<secret kind>`.
- Held in memory as `SecretString` (zeroised on drop, redacted `Debug`).
- Never written to the store database, logs, error messages, history or exports.
- Never returned to the frontend. Edit forms show "stored" and accept a replacement.
- Private keys are referenced by path and read at connect time; the app does not copy them.
- Deleting a connection deletes its keychain entries.
- Keychain access goes through `keyring-core` with a store per platform, chosen at startup (the `keyring` crate itself is only a sample CLI since v4). macOS uses `apple-native-keyring-store` with the `keychain` feature; Linux and Windows stores are added with packaging (Phase 8). The store database records only *whether* a secret is saved (`has_db_password`), so listing connections never touches the keychain.

## SSH

- Host keys are checked against the user's `~/.ssh/known_hosts` (read-only) and the app's own `known_hosts` table.
- Unknown host: the connection stops with `ssh_host_key_unknown` and the fingerprint. Only an explicit user decision stores the key, in the app's table.
- Changed host key: hard failure (`ssh_host_key_changed`). There is no "connect anyway" button; the user must remove the stored key deliberately in settings.
- Authentication order is whatever the profile specifies; there is no silent fallback from key to password.
- The forwarding listener binds to `127.0.0.1` only, on an OS-assigned port, and closes with the session. Other local processes can reach it while it is open, as with `ssh -L`; this is accepted.

## TLS

- Modes follow PostgreSQL's vocabulary: `disable`, `prefer`, `require`, `verify-ca`, `verify-full`. The default for new remote connections is `verify-full`; for `localhost` it is `prefer`.
- Through a tunnel, certificate verification uses the real server name.
- `rustls` with the platform's root store, plus an optional CA file per connection.
- Choosing a mode that does not verify the certificate shows a visible warning on the connection form.

## Generated SQL

- Values are always bound parameters.
- Identifiers always pass through `Dialect::quote_identifier`, which escapes the quote character. Table and column names come from introspection, but are still treated as untrusted.
- `Dialect::render_literal` output is for display and file export only and is never executed by the app.
- The grid filter is a raw `WHERE` expression typed by the user. It is their own SQL on their own connection, equivalent to typing it in the editor; it is passed through `sql::classify`-style checks to reject a `;` that would start a second statement.

## Production safeguards

1. **Read-only connections.** Enforced twice: the server session is set read-only where supported, and the backend rejects statements classified as anything other than `Read`, `Session` or `Transaction` before sending. `Unknown` counts as a write, and so do session or transaction statements that would switch the session back to read-write (`SET ... READ WRITE`, `SET default_transaction_read_only = off`, `RESET ALL`).
2. **Dangerous statements** (`UPDATE`/`DELETE` without `WHERE`, `DROP`, `TRUNCATE`) return `confirmation_required`. The check is in the backend, so the frontend cannot skip it by mistake.
3. **Colour tag** of the active connection is always visible in the tab and status bar.
4. **Staged edits** are never applied without the SQL preview.

The classifier is a heuristic. It is a seatbelt against accidents, not an access control mechanism; real protection is a read-only database role.

## Webview

- Database values are rendered as text nodes only. No `dangerouslySetInnerHTML`, no HTML rendering of cell content, including in the JSON and XML viewers.
- Strict Content Security Policy: no remote scripts, no inline scripts, no `eval`.
- Minimal Tauri capabilities (see IPC.md). No shell plugin.
- No network requests from the frontend.

## Logging

- Structured logs (`tracing`) to a file in the app log directory, rotated.
- Logged: command names, durations, error codes, driver kind.
- Not logged: SQL text, parameters, cell values, hostnames with credentials, secrets. SQL text lives only in the history table, which the user can clear.

## Dependencies

- `cargo audit` and `pnpm audit` run in CI.
- No `unsafe` in workspace crates.
- SQLite is bundled, so its version is controlled by the app.

## Checklist before each phase is closed

- [ ] No secret in logs, errors, store database or IPC responses (grep the diff for password fields).
- [ ] All generated SQL uses bound parameters and quoted identifiers.
- [ ] New commands validate their arguments and return `AppError`, never a panic.
- [ ] New UI renders database content as text only.
- [ ] `cargo audit` and `pnpm audit` are clean or exceptions are recorded.
