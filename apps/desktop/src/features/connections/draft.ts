import type {
  ConnectionDraft,
  ConnectionProfile,
  ConnectionSecrets,
  DriverInfo,
  TlsMode,
} from "../../ipc";

export const DEFAULT_COLOR = "#3b82f6";

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
  };
}

/** The editable part of a saved profile; `hasDbPassword` is the backend's to report. */
export function draftFromProfile(profile: ConnectionProfile): ConnectionDraft {
  return {
    id: profile.id,
    name: profile.name,
    driver: profile.driver,
    color: profile.color,
    readOnly: profile.readOnly,
    host: profile.host,
    port: profile.port,
    database: profile.database,
    user: profile.user,
    filePath: profile.filePath,
    options: { ...profile.options },
    tls: { ...profile.tls },
  };
}

/** What the password field means for the backend: keep, replace or forget. */
export type PasswordChange = { kind: "keep" } | { kind: "set"; value: string } | { kind: "clear" };

export function secretsFor(change: PasswordChange): ConnectionSecrets | undefined {
  switch (change.kind) {
    case "keep":
      return undefined;
    case "set":
      return { dbPassword: change.value };
    case "clear":
      return { dbPassword: null };
  }
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
  return problems;
}
