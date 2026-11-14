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
  QueryExecuteArgs,
  QueryResult,
  StatementRange,
  Value,
} from "../ipc/generated";
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
    if (trimmed) ranges.push({ start: start + leading, end: start + leading + trimmed.length });
    start += part.length + 1;
  }
  return ranges;
}

/** A statement that runs until it is cancelled, like `SELECT pg_sleep(30)`. */
const SLEEP = /sleep\(/i;

export function createMockBackend() {
  const profiles = new Map<string, ConnectionProfile>();
  const sessions = new Map<string, ConnectionProfile>();
  /** Pending sleeps by tab, resolved by `query_cancel`. */
  const sleeping = new Map<string, () => void>();
  let nextId = 1;

  const save = (draft: ConnectionDraft, secrets?: ConnectionSecrets): ConnectionProfile => {
    if (!draft.name.trim())
      throw { code: "invalid_argument", message: "The connection name is required" };
    const id = draft.id ?? `c${nextId++}`;
    const previous = profiles.get(id);
    const password = secrets?.dbPassword;
    const profile: ConnectionProfile = {
      ...draft,
      id,
      name: draft.name.trim(),
      hasDbPassword:
        password === undefined ? (previous?.hasDbPassword ?? false) : password !== null,
    };
    profiles.set(id, profile);
    return profile;
  };

  const execute = (args: QueryExecuteArgs): Promise<QueryResult> => {
    const profile = sessions.get(args.sessionId);
    if (!profile) return reject("not_found", "The session was not found");
    const sql = args.sql.trim();
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
      const stored = draft.id !== null && profiles.get(draft.id)?.hasDbPassword === true;
      if (!typed && !stored) return reject("auth", "password authentication failed");
      return { serverVersion: "PostgreSQL 18 (mock)", latencyMs: 2 };
    },
    session_open: (a) => {
      const profile = profiles.get(a.connectionId as string);
      if (!profile) return reject("not_found", "The connection was not found");
      const info = sessionFor(profile, `s-${profile.id}`);
      sessions.set(info.sessionId, profile);
      return info;
    },
    session_close: (a) => {
      sessions.delete(a.sessionId as string);
      return null;
    },
    sql_split: (a) => split(a.text as string),
    query_execute: (a) => execute(a as unknown as QueryExecuteArgs),
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
