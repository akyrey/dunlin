import type { Capabilities, ConnectionProfile, DriverInfo, SessionInfo, TlsSettings } from "../ipc";

export const sqliteCapabilities: Capabilities = {
  schemas: false,
  multipleDatabases: false,
  cancel: true,
  readOnlySession: true,
  transactionalDdl: true,
  explain: true,
  explainAnalyze: true,
  explainAnalyzeWrites: true,
  schemaEditing: true,
  tableStats: true,
  tls: false,
  tunnel: false,
  keyValue: false,
};

const networkCapabilities: Capabilities = { ...sqliteCapabilities, tls: true, tunnel: true };

export const noTls: TlsSettings = { mode: "disable", caFile: null, certFile: null, keyFile: null };

export const sqliteDriver: DriverInfo = {
  kind: "sqlite",
  label: "SQLite",
  endpoint: "file",
  defaultPort: null,
  tlsModes: [],
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

export const postgresDriver: DriverInfo = {
  kind: "postgres",
  label: "PostgreSQL",
  endpoint: "tcp",
  defaultPort: 5432,
  tlsModes: ["disable", "prefer", "require", "verify-ca", "verify-full"],
  capabilities: { ...networkCapabilities, schemas: true },
  options: [],
};

export const mysqlDriver: DriverInfo = {
  kind: "mysql",
  label: "MySQL / MariaDB",
  endpoint: "tcp",
  defaultPort: 3306,
  tlsModes: ["disable", "prefer", "require", "verify-full"],
  capabilities: { ...networkCapabilities, multipleDatabases: true },
  options: [],
};

export const redisDriver: DriverInfo = {
  kind: "redis",
  label: "Redis / Valkey",
  endpoint: "tcp",
  defaultPort: 6379,
  tlsModes: ["disable", "require", "verify-ca", "verify-full"],
  capabilities: {
    ...networkCapabilities,
    explain: false,
    explainAnalyze: false,
    explainAnalyzeWrites: false,
    tableStats: false,
    schemaEditing: false,
    transactionalDdl: false,
    readOnlySession: false,
    keyValue: true,
  },
  options: [],
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
    tls: noTls,
    ssh: null,
    savedSecrets: [],
    alwaysPreview: false,
    folder: null,
    ...overrides,
  };
}

export function postgresProfile(overrides: Partial<ConnectionProfile> = {}): ConnectionProfile {
  return sqliteProfile({
    id: "pg1",
    name: "Staging",
    driver: "postgres",
    host: "db.example.com",
    port: 5432,
    database: "app",
    user: "app",
    filePath: null,
    options: {},
    tls: { ...noTls, mode: "verify-full" },
    savedSecrets: ["db_password"],
    ...overrides,
  });
}

export function sessionFor(profile: ConnectionProfile, sessionId = "s1"): SessionInfo {
  return {
    sessionId,
    connectionId: profile.id,
    driver: profile.driver,
    capabilities: profile.driver === "redis" ? redisDriver.capabilities : sqliteCapabilities,
    readOnly: profile.readOnly,
    defaultNamespace: { catalog: null, schema: "main" },
  };
}
