import type {
  ConnectionDraft,
  ConnectionProfile,
  ConnectionSecrets,
  DriverInfo,
  SecretKind,
  SshHostSettings,
  SshSettings,
  TlsMode,
} from "../../ipc";

export const DEFAULT_COLOR = "#3b82f6";
export const DEFAULT_SSH_PORT = 22;

const LOCAL_HOSTS = new Set(["localhost", "127.0.0.1", "::1"]);

/** Modes that encrypt without checking the certificate; the form warns about them. */
export function isUnverified(mode: TlsMode): boolean {
  return mode === "prefer" || mode === "require";
}

/**
 * SECURITY.md: `verify-full` for new remote connections, `prefer` for localhost, limited to what
 * the driver supports.
 */
export function defaultTlsMode(driver: DriverInfo, host: string | null): TlsMode {
  const preferred: TlsMode[] = LOCAL_HOSTS.has((host ?? "").trim().toLowerCase())
    ? ["prefer", "require", "disable"]
    : ["verify-full", "require", "prefer", "disable"];
  return preferred.find((mode) => driver.tlsModes.includes(mode)) ?? "disable";
}

/** A new, empty profile for `driver`, with its options at their declared defaults. */
export function emptyDraft(driver: DriverInfo): ConnectionDraft {
  const options: Record<string, string> = {};
  for (const option of driver.options) {
    if (option.default !== null) options[option.key] = option.default;
  }
  const network = driver.endpoint === "tcp";
  return {
    id: null,
    name: "",
    driver: driver.kind,
    color: DEFAULT_COLOR,
    readOnly: false,
    host: network ? "localhost" : null,
    port: driver.defaultPort,
    database: null,
    user: null,
    filePath: null,
    options,
    tls: {
      mode: network ? defaultTlsMode(driver, "localhost") : "disable",
      caFile: null,
      certFile: null,
      keyFile: null,
    },
    ssh: null,
    alwaysPreview: false,
    folder: null,
  };
}

export function emptySshHost(): SshHostSettings {
  return { host: "", port: DEFAULT_SSH_PORT, user: "", auth: "agent", keyFile: null };
}

export function emptySsh(): SshSettings {
  return { server: emptySshHost(), jump: null };
}

/** The editable part of a saved profile; which secrets are stored is the backend's to report. */
export function draftFromProfile(profile: ConnectionProfile): ConnectionDraft {
  return {
    id: profile.id,
    name: profile.name,
    driver: profile.driver,
    color: profile.color,
    readOnly: profile.readOnly,
    alwaysPreview: profile.alwaysPreview,
    folder: profile.folder,
    host: profile.host,
    port: profile.port,
    database: profile.database,
    user: profile.user,
    filePath: profile.filePath,
    options: { ...profile.options },
    tls: { ...profile.tls },
    ssh: profile.ssh
      ? {
          server: { ...profile.ssh.server },
          jump: profile.ssh.jump ? { ...profile.ssh.jump } : null,
        }
      : null,
  };
}

/** What a secret field means for the backend: keep, replace or forget. */
export type PasswordChange = { kind: "keep" } | { kind: "set"; value: string } | { kind: "clear" };

export type SecretField = keyof ConnectionSecrets;

export const SECRET_KINDS: Record<SecretField, SecretKind> = {
  dbPassword: "db_password",
  sshPassword: "ssh_password",
  sshKeyPassphrase: "ssh_key_passphrase",
  jumpPassword: "jump_password",
  jumpKeyPassphrase: "jump_key_passphrase",
};

export type SecretChanges = Partial<Record<SecretField, PasswordChange>>;

function secretValue(change: PasswordChange | undefined): string | null | undefined {
  switch (change?.kind) {
    case undefined:
    case "keep":
      return undefined;
    case "set":
      return change.value;
    case "clear":
      return null;
  }
}

/** Only changed fields are sent: absent keeps the stored secret (docs/IPC.md). */
export function secretsFor(changes: SecretChanges): ConnectionSecrets | undefined {
  const secrets: ConnectionSecrets = {};
  for (const field of Object.keys(SECRET_KINDS) as SecretField[]) {
    const value = secretValue(changes[field]);
    if (value !== undefined) secrets[field] = value;
  }
  return Object.keys(secrets).length > 0 ? secrets : undefined;
}

function sshHostProblems(host: SshHostSettings, label: string): string[] {
  const problems: string[] = [];
  if (host.host.trim() === "") problems.push(`${label}: enter a host`);
  if (host.user.trim() === "") problems.push(`${label}: enter a user`);
  if (host.auth === "key_file" && !host.keyFile?.trim())
    problems.push(`${label}: choose a key file`);
  return problems;
}

/** Client-side checks that mirror the backend's, for immediate feedback. */
export function draftProblems(draft: ConnectionDraft, driver: DriverInfo): string[] {
  const problems: string[] = [];
  if (draft.name.trim() === "") problems.push("Enter a name");
  if (driver.endpoint === "file" && !draft.filePath?.trim())
    problems.push("Choose a database file");
  if (driver.endpoint === "tcp" && !draft.host?.trim()) problems.push("Enter a host");
  if (Boolean(draft.tls.certFile) !== Boolean(draft.tls.keyFile)) {
    problems.push("A client certificate needs both a certificate and a key file");
  }
  if (draft.ssh) {
    problems.push(...sshHostProblems(draft.ssh.server, "SSH"));
    if (draft.ssh.jump) problems.push(...sshHostProblems(draft.ssh.jump, "Jump host"));
  }
  return problems;
}
