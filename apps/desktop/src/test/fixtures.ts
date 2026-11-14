import type { Capabilities, ConnectionProfile, DriverInfo, SessionInfo } from "../ipc";

export const sqliteCapabilities: Capabilities = {
  schemas: false,
  multipleDatabases: false,
  cancel: true,
  readOnlySession: true,
  transactionalDdl: true,
  explain: true,
  tls: false,
  tunnel: false,
};

export const sqliteDriver: DriverInfo = {
  kind: "sqlite",
  label: "SQLite",
  endpoint: "file",
  capabilities: sqliteCapabilities,
  options: [
    {
      key: "create_if_missing",
      label: "Create the file if it does not exist",
      kind: "bool",
      default: "false",
      help: null,
    },
  ],
};

export function sqliteProfile(overrides: Partial<ConnectionProfile> = {}): ConnectionProfile {
  return {
    id: "c1",
    name: "Local",
    driver: "sqlite",
    color: "#22aa88",
    readOnly: false,
    host: null,
    port: null,
    database: null,
    user: null,
    filePath: "/tmp/local.db",
    options: { create_if_missing: "false" },
    ...overrides,
  };
}

export function sessionFor(profile: ConnectionProfile, sessionId = "s1"): SessionInfo {
  return {
    sessionId,
    connectionId: profile.id,
    driver: profile.driver,
    capabilities: sqliteCapabilities,
    readOnly: profile.readOnly,
  };
}
