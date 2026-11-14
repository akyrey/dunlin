/**
 * An in-memory stand-in for the Rust backend, used by Playwright (DECISIONS D-011) and App tests.
 * It follows the same contract as the real commands; keep it in step with docs/IPC.md.
 */
import { mockIPC } from "@tauri-apps/api/mocks";

import type {
  AppError,
  ColumnMeta,
  ConnectionDraft,
  ConnectionProfile,
  ConnectionSecrets,
  HistoryEntry,
  HistoryListArgs,
  KnownHost,
  Namespace,
  ObjectSummary,
  QueryExecuteArgs,
  QueryResult,
  SecretKind,
  StatementRange,
  TableInfo,
  TableRef,
  Value,
} from "../ipc/generated";
import { SECRET_KINDS, type SecretField } from "../features/connections/draft";
import { mysqlDriver, postgresDriver, sessionFor, sqliteDriver } from "./fixtures";

export const MOCK_DATABASE_PATH = "/mock/people.db";
export const MOCK_NEW_DATABASE_PATH = "/mock/new.db";
const MOCK_ROW_LIMIT_TABLE = /^select \* from big/i;

type Args = Record<string, unknown>;

const peopleColumns: ColumnMeta[] = [
  { name: "id", typeName: "INTEGER", category: "integer", nullable: null },
  { name: "name", typeName: "TEXT", category: "text", nullable: null },
  { name: "balance", typeName: "BIGINT", category: "integer", nullable: null },
];

const peopleRows: Value[][] = [
  [
    { t: "int", v: "1" },
    { t: "text", v: "Ada" },
    { t: "int", v: "9007199254740993" },
  ],
  [{ t: "int", v: "2" }, { t: "text", v: "Grace" }, { t: "null" }],
  [
    { t: "int", v: "3" },
    { t: "text", v: "" },
    { t: "int", v: "0" },
  ],
];

function reject(code: string, message: string, detail?: unknown): Promise<never> {
  const error: AppError = detail === undefined ? { code, message } : { code, message, detail };
  return Promise.reject(error);
}

function rows(columns: ColumnMeta[], data: Value[][], truncated = false): QueryResult {
  return {
    outcome: { kind: "rows", columns, rows: data, truncated },
    elapsedMs: 3,
    statementClass: "read",
  };
}

function split(text: string): StatementRange[] {
  const ranges: StatementRange[] = [];
  let start = 0;
  for (const part of text.split(";")) {
    const leading = part.length - part.trimStart().length;
    const trimmed = part.trim();
    if (trimmed) {
      const danger = /^delete from \w+$/i.test(trimmed)
        ? "DELETE without WHERE affects every row"
        : null;
      ranges.push({ start: start + leading, end: start + leading + trimmed.length, danger });
    }
    start += part.length + 1;
  }
  return ranges;
}

const MAIN = { catalog: null, schema: "main" };
const MOCK_NAMESPACES: Namespace[] = [
  { namespace: MAIN, name: "main", system: false },
  { namespace: { catalog: null, schema: "temp" }, name: "temp", system: true },
];
const MOCK_OBJECTS: ObjectSummary[] = [
  { name: "orders", kind: "table", comment: null },
  { name: "people", kind: "table", comment: "Everyone we know" },
  { name: "rich_people", kind: "view", comment: null },
];

const column = (name: string, typeName: string, nullable: boolean, ordinal: number) => ({
  name,
  typeName,
  category: typeName === "TEXT" ? ("text" as const) : ("integer" as const),
  nullable,
  default: null,
  comment: null,
  ordinal,
  generated: false,
});

function mockTable(table: TableRef): TableInfo {
  const base = {
    table,
    kind: "table" as const,
    comment: null,
    primaryKey: null,
    indexes: [],
    foreignKeys: [],
    uniqueConstraints: [],
    checkConstraints: [],
    rowKey: null,
  };
  switch (table.name) {
    case "people":
      return {
        ...base,
        comment: "Everyone we know",
        columns: [
          column("id", "INTEGER", false, 1),
          column("name", "TEXT", false, 2),
          column("balance", "BIGINT", true, 3),
        ],
        primaryKey: { name: null, columns: ["id"] },
        indexes: [
          {
            name: "people_name_idx",
            columns: ["name"],
            unique: false,
            method: null,
            predicate: null,
          },
        ],
        checkConstraints: [{ name: "balance_ok", expression: "balance >= 0" }],
        rowKey: ["id"],
      };
    case "orders":
      return {
        ...base,
        columns: [column("id", "INTEGER", false, 1), column("person_id", "INTEGER", false, 2)],
        primaryKey: { name: null, columns: ["id"] },
        foreignKeys: [
          {
            name: null,
            columns: ["person_id"],
            referencedTable: { namespace: MAIN, name: "people" },
            referencedColumns: ["id"],
            onUpdate: null,
            onDelete: "cascade",
          },
        ],
        rowKey: ["id"],
      };
    default:
      return { ...base, kind: "view", columns: [column("name", "TEXT", true, 1)] };
  }
}

const quote = (name: string) => `"${name.replaceAll('"', '""')}"`;

/** The SSH server's fingerprint in the mock; a host named `changed.example.com` presents another. */
export const MOCK_FINGERPRINT = "SHA256:mockmockmockmockmockmockmockmockmockmockmock";
/** Running this statement makes the mock behave as if the SSH tunnel dropped. */
export const MOCK_DROP_TUNNEL = "SELECT drop_tunnel()";

/** A statement that runs until it is cancelled, like `SELECT pg_sleep(30)`. */
const SLEEP = /sleep\(/i;

export function createMockBackend() {
  const profiles = new Map<string, ConnectionProfile>();
  const sessions = new Map<string, ConnectionProfile>();
  /** Pending sleeps by tab, resolved by `query_cancel`. */
  const sleeping = new Map<string, () => void>();
  const knownHosts: KnownHost[] = [];
  const history: HistoryEntry[] = [];

  /** Records a statement the way the backend does after running it. */
  const record = (args: QueryExecuteArgs, outcome: Promise<QueryResult>): Promise<QueryResult> => {
    const connectionId = sessions.get(args.sessionId)?.id ?? "";
    const entry = (patch: Partial<HistoryEntry>): HistoryEntry => ({
      id: history.length + 1,
      connectionId,
      statement: args.sql,
      startedAt: Date.now(),
      durationMs: 1,
      outcome: "rows",
      rowCount: null,
      truncated: false,
      error: null,
      ...patch,
    });
    outcome.then(
      (result) => {
        history.push(
          entry(
            result.outcome.kind === "rows"
              ? {
                  outcome: "rows",
                  rowCount: result.outcome.rows.length,
                  truncated: result.outcome.truncated,
                }
              : { outcome: "affected", rowCount: Number(result.outcome.rows) },
          ),
        );
      },
      (caught: unknown) => {
        const error = caught as AppError;
        if (error.code !== "confirmation_required" && error.code !== "read_only") {
          history.push(entry({ outcome: "error", error: error.message }));
        }
      },
    );
    return outcome;
  };
  let nextId = 1;

  const save = (draft: ConnectionDraft, secrets?: ConnectionSecrets): ConnectionProfile => {
    if (!draft.name.trim())
      throw { code: "invalid_argument", message: "The connection name is required" };
    const id = draft.id ?? `c${nextId++}`;
    const saved = new Set(profiles.get(id)?.savedSecrets ?? []);
    for (const [field, kind] of Object.entries(SECRET_KINDS) as [SecretField, SecretKind][]) {
      const value = secrets?.[field];
      if (value === null) saved.delete(kind);
      else if (value !== undefined) saved.add(kind);
    }
    const profile: ConnectionProfile = {
      ...draft,
      id,
      name: draft.name.trim(),
      savedSecrets: [...saved].sort(),
    };
    profiles.set(id, profile);
    return profile;
  };

  /** The SSH trust flow: unknown until trusted; `changed.example.com` always presents another key. */
  const checkHostKey = (profile: ConnectionProfile): Promise<never> | null => {
    const server = profile.ssh?.server;
    if (!server) return null;
    const presented = server.host === "changed.example.com" ? "SHA256:other" : MOCK_FINGERPRINT;
    const detail = {
      stage: "host_key",
      host: server.host,
      port: server.port,
      algorithm: "ssh-ed25519",
      fingerprint: presented,
    };
    const trusted = knownHosts.find((k) => k.host === server.host && k.port === server.port);
    if (!trusted)
      return reject(
        "ssh_host_key_unknown",
        `The SSH host key of ${server.host} is not trusted yet`,
        detail,
      );
    if (trusted.fingerprint !== presented) {
      return reject(
        "ssh_host_key_changed",
        `The SSH host key of ${server.host} has changed`,
        detail,
      );
    }
    return null;
  };

  const execute = (args: QueryExecuteArgs): Promise<QueryResult> => {
    const profile = sessions.get(args.sessionId);
    if (!profile) return reject("not_found", "The session was not found");
    const sql = args.sql.trim();
    if (sql === MOCK_DROP_TUNNEL) {
      sessions.delete(args.sessionId);
      return reject("tunnel_lost", "The SSH connection was lost");
    }
    if (SLEEP.test(sql)) {
      return new Promise((_resolve, rejectSleep) => {
        sleeping.set(args.tabId, () => {
          rejectSleep({ code: "cancelled", message: "canceling statement due to user request" });
        });
      });
    }
    if (/FRM/.test(sql))
      return reject("syntax", 'near "FRM": syntax error', { position: sql.indexOf("FRM") });
    if (/^(insert|update|delete|create|drop)/i.test(sql) && profile.readOnly) {
      return reject("read_only", "This connection is read-only; the statement would modify data");
    }
    if (/^delete from \w+$/i.test(sql) && args.confirmed !== true) {
      return reject("confirmation_required", "DELETE without WHERE affects every row", {
        reason: "DELETE without WHERE affects every row",
      });
    }
    if (/^(insert|update|delete)/i.test(sql)) {
      return Promise.resolve({
        outcome: { kind: "affected", rows: "2" },
        elapsedMs: 1,
        statementClass: "write",
      });
    }
    if (MOCK_ROW_LIMIT_TABLE.test(sql)) {
      const data: Value[][] = Array.from({ length: args.rowLimit }, (_, i) => [
        { t: "int", v: String(i + 1) },
      ]);
      return Promise.resolve(rows([peopleColumns[0] as ColumnMeta], data, true));
    }
    if (/^select \* from people/i.test(sql))
      return Promise.resolve(rows(peopleColumns, peopleRows));
    return Promise.resolve(
      rows(
        [{ name: "?column?", typeName: "", category: "integer", nullable: null }],
        [[{ t: "int", v: "1" }]],
      ),
    );
  };

  const handlers: Record<string, (args: Args) => unknown> = {
    driver_list: () => [postgresDriver, mysqlDriver, sqliteDriver],
    connection_list: () => [...profiles.values()],
    connection_save: (a) =>
      save(a.profile as ConnectionDraft, a.secrets as ConnectionSecrets | undefined),
    connection_delete: (a) => {
      profiles.delete(a.connectionId as string);
      return null;
    },
    connection_test: (a) => {
      const draft = a.profile as ConnectionDraft;
      if (draft.filePath?.includes("missing"))
        return reject("connect", "unable to open database file");
      if (draft.driver === "sqlite") return { serverVersion: "SQLite 3 (mock)", latencyMs: 1 };
      const typed = (a.secrets as ConnectionSecrets | undefined)?.dbPassword;
      const stored =
        draft.id !== null && profiles.get(draft.id)?.savedSecrets.includes("db_password") === true;
      if (!typed && !stored) {
        return reject("auth", "password authentication failed", { stage: "db_auth" });
      }
      return { serverVersion: "PostgreSQL 18 (mock)", latencyMs: 2 };
    },
    session_open: (a) => {
      const profile = profiles.get(a.connectionId as string);
      if (!profile) return reject("not_found", "The connection was not found");
      const hostKeyError = checkHostKey(profile);
      if (hostKeyError) return hostKeyError;
      const info = sessionFor(profile, `s-${profile.id}`);
      sessions.set(info.sessionId, profile);
      return info;
    },
    session_close: (a) => {
      sessions.delete(a.sessionId as string);
      return null;
    },
    sql_split: (a) => split(a.text as string),
    query_execute: (a) =>
      record(a as unknown as QueryExecuteArgs, execute(a as unknown as QueryExecuteArgs)),
    history_list: (a) => {
      const args = a as unknown as HistoryListArgs;
      const search = args.search?.toLowerCase();
      return history
        .filter((e) => e.connectionId === args.connectionId)
        .filter((e) => !search || e.statement.toLowerCase().includes(search))
        .filter((e) => args.before === undefined || e.id < args.before)
        .reverse()
        .slice(0, args.limit);
    },
    history_clear: (a) => {
      const keep = history.filter((e) => e.connectionId !== a.connectionId);
      history.splice(0, history.length, ...keep);
      return null;
    },
    schema_completion: () => ({
      tables: MOCK_OBJECTS.map((o) => ({
        schema: "main",
        name: o.name,
        columns: mockTable({ namespace: MAIN, name: o.name }).columns.map((c) => c.name),
      })),
    }),
    schema_namespaces: () => MOCK_NAMESPACES,
    schema_objects: () => MOCK_OBJECTS,
    schema_refresh: () => null,
    schema_table: (a) => {
      const table = a.table as TableRef;
      if (!MOCK_OBJECTS.some((o) => o.name === table.name)) {
        return reject("not_found", `${table.name} does not exist`);
      }
      return mockTable(table);
    },
    schema_table_ddl: (a) => {
      const info = mockTable(a.table as TableRef);
      const columns = info.columns.map((c) => `    ${c.name} ${c.typeName}`).join(",\n");
      return `CREATE TABLE ${info.table.name} (\n${columns}\n);\n`;
    },
    schema_snippets: (a) => {
      const table = a.table as TableRef;
      const qualifiedName = `${quote(table.namespace.schema ?? "main")}.${quote(table.name)}`;
      const columns = mockTable(table)
        .columns.map((c) => quote(c.name))
        .join(", ");
      return { qualifiedName, select: `SELECT ${columns}\nFROM ${qualifiedName};` };
    },
    ssh_trust_host: (a) => {
      knownHosts.push(a as unknown as KnownHost);
      return null;
    },
    ssh_known_hosts_list: () => [...knownHosts],
    ssh_known_host_remove: (a) => {
      const index = knownHosts.findIndex((k) => k.host === a.host && k.port === a.port);
      if (index >= 0) knownHosts.splice(index, 1);
      return null;
    },
    query_cancel: (a) => {
      sleeping.get(a.tabId as string)?.();
      sleeping.delete(a.tabId as string);
      return null;
    },
    tab_close: () => null,
    "plugin:dialog|open": () => MOCK_DATABASE_PATH,
    "plugin:dialog|save": () => MOCK_NEW_DATABASE_PATH,
  };

  return (cmd: string, payload?: unknown): unknown => {
    const handler = handlers[cmd];
    if (!handler) return reject("internal", `Mock backend has no command ${cmd}`);
    const envelope = (payload ?? {}) as { args?: Args; options?: Args };
    try {
      return handler(envelope.args ?? envelope.options ?? {});
    } catch (error) {
      return Promise.reject(error);
    }
  };
}

export function installMockBackend(): void {
  mockIPC(createMockBackend());
}
