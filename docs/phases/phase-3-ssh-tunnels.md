# Phase 3: SSH tunnels

## Goal

Any PostgreSQL or MySQL connection can be routed through an SSH server, with host key verification, three authentication methods and an optional jump host.

## Read first

[ARCHITECTURE.md](../ARCHITECTURE.md#ssh-tunnels), [SECURITY.md](../SECURITY.md#ssh), DECISIONS D-004.

## In scope

- `dbc-tunnel` crate.
- Tunnel configuration on connection profiles, with secrets in the keychain.
- Host key verification and the trust prompt.
- Tunnel lifecycle tied to the session; keepalive and loss detection.
- Cancellation side-connections routed through the tunnel.

## Out of scope

Reading `~/.ssh/config`, remote and dynamic forwarding, more than one jump host, SSH certificates, hardware-backed keys beyond what the agent provides.

## Design

```rust
pub struct TunnelConfig {
    pub ssh_host: String,
    pub ssh_port: u16,
    pub user: String,
    pub auth: SshAuth,                 // Agent | KeyFile { path, passphrase } | Password(SecretString)
    pub jump: Option<Box<TunnelConfig>>,
    pub target_host: String,           // database host as seen from the SSH server
    pub target_port: u16,
    pub keepalive: Duration,
}

pub struct Tunnel { /* owns the SSH session and listener task */ }

impl Tunnel {
    pub async fn open(config: &TunnelConfig, hosts: &dyn HostKeyStore) -> Result<Tunnel, TunnelError>;
    pub fn local_addr(&self) -> SocketAddr;      // 127.0.0.1:<assigned port>
    pub async fn close(self) -> Result<(), TunnelError>;
}

pub trait HostKeyStore: Send + Sync {
    fn check(&self, host: &str, port: u16, key: &PublicKey) -> HostKeyStatus;   // Trusted | Unknown | Changed
}
```

`TunnelError` distinguishes `HostKeyUnknown { fingerprint }`, `HostKeyChanged`, `Auth`, `Connect`, `ForwardRefused`, `Lost`.

## Tasks

- [x] 3.1 **Spike: `russh`.** Against a containerised OpenSSH server, prove: connect, verify host key in the handler, authenticate by key, open a `direct-tcpip` channel, pipe a TCP connection through it. Check the current API in its docs; note anything that differs from this brief.
- [x] 3.2 Integration harness: OpenSSH container plus a database container on a private Docker network, so the database is reachable only through SSH.
- [x] 3.3 `Tunnel::open` with key-file authentication, loopback listener, one forwarding task per accepted socket, clean shutdown.
- [x] 3.4 Password authentication and agent authentication (`SSH_AUTH_SOCK`). Encrypted private keys with passphrase.
- [x] 3.5 `HostKeyStore`: composite of the user's `~/.ssh/known_hosts` (read-only, including hashed entries) and the app's `known_hosts` table. Tests for trusted, unknown, changed.
- [x] 3.6 Store migration for `known_hosts`; `ssh_trust_host` command; settings entry to list and remove trusted hosts.
- [x] 3.7 Jump host: second SSH session over a channel of the first. Integration test with two SSH containers.
- [x] 3.8 Keepalive and loss detection: when the SSH connection drops, close the listener and surface `tunnel_lost` on the next database call.
- [x] 3.9 Session integration: open tunnel before connecting; pass `dial_host`/`dial_port` as the loopback address and keep `server_name` as the real host; route PostgreSQL cancel connections through the tunnel; close tunnel after connections.
- [x] 3.10 Profile model and store migration for tunnel settings; secret kinds `ssh_password` and `ssh_key_passphrase`.
- [x] 3.11 Frontend: SSH section in the connection form (toggle, host, port, user, auth method, key file picker, jump host subsection); host key trust dialog showing the SHA-256 fingerprint; clear error for a changed key.
- [x] 3.12 "Test connection" reports which stage failed: SSH connect, host key, SSH auth, forward, database connect, database auth.

## Acceptance criteria

- The conformance suite passes for PostgreSQL and MySQL when run through a tunnel.
- A database that is not reachable directly is reachable through the tunnel and through a jump host.
- First connection to an unknown host prompts with the fingerprint; declining leaves nothing stored; accepting stores it and the next connection does not prompt.
- A changed host key is refused with no way to proceed from the dialog.
- TLS `verify-full` to the database works through a tunnel.
- Cancelling a PostgreSQL query works through a tunnel.
- Killing the SSH server mid-session produces `tunnel_lost`, and reopening the session works once it is back.
- No listener remains bound after the session closes (asserted in a test).
- Coverage at or above 80% for `dbc-tunnel`.

## Manual smoke

1. Connect to a real database behind a real bastion using the SSH agent.
2. Same with a passphrase-protected key file.
3. Leave the session idle for 15 minutes, then run a query.
