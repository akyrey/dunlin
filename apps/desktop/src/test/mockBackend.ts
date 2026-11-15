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
  KeyArg,
  OperationArg,
  SavedQuery,
  SavedQueryDraft,
  Settings,
  SortKey,
  TablePage,
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
import { BOUNDS, DEFAULT_SETTINGS } from "../features/settings/defaults";
import { SECRET_KINDS, type SecretField } from "../features/connections/draft";
import { rawText } from "../lib/value";
import { mysqlDriver, postgresDriver, sessionFor, sqliteDriver } from "./fixtures";

export const MOCK_DATABASE_PATH = "/mock/people.db";
export const MOCK_NEW_DATABASE_PATH = "/mock/new.db";
const MOCK_ROW_LIMIT_TABLE = /^select \* from big/i;
/** 1000 rows × 50 columns of mixed types, for the grid performance check (task 6.1). */
const MOCK_WIDE_TABLE = /^select \* from wide/i;
const WIDE_ROWS = 1000;
const WIDE_COLUMNS = 50;

function wideResult(): QueryResult {
  const kinds = ["integer", "text", "decimal", "json", "bool"] as const;
  const columns: ColumnMeta[] = Array.from({ length: WIDE_COLUMNS }, (_, c) => ({
    name: `column_${c}`,
    typeName: kinds[c % kinds.length] ?? "text",
    category: kinds[c % kinds.length] ?? "text",
    nullable: true,
  }));
  const cell = (r: number, c: number): Value => {
    switch (c % kinds.length) {
      case 0:
        return { t: "int", v: String(r * WIDE_COLUMNS + c) };
      case 1:
        return { t: "text", v: `row ${r} column ${c} with some longer text to fill the cell` };
      case 2:
        return { t: "decimal", v: `${r}.${String(c).padStart(2, "0")}` };
      case 3:
        return { t: "json", v: `{"row": ${r}, "tags": ["a", "b"], "nested": {"c": ${c}}}` };
      default:
        return r % 7 === 0 ? { t: "null" } : { t: "bool", v: r % 2 === 0 };
    }
  };
  const data = Array.from({ length: WIDE_ROWS }, (_, r) =>
    Array.from({ length: WIDE_COLUMNS }, (_, c) => cell(r, c)),
  );
  return rows(columns, data);
}

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

function rows(
  columns: ColumnMeta[],
  data: Value[][],
  truncated = false,
  source: string | null = null,
): QueryResult {
  return {
    outcome: { kind: "rows", columns, rows: data, truncated },
    elapsedMs: 3,
    statementClass: "read",
    source: source === null ? null : { namespace: MAIN, name: source },
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
const NO_TRANSACTION = { manual: false, open: false };
const MOCK_NAMESPACES: Namespace[] = [
  { namespace: MAIN, name: "main", system: false },
  { namespace: { catalog: null, schema: "temp" }, name: "temp", system: true },
];
const MOCK_OBJECTS: ObjectSummary[] = [
  { name: "documents", kind: "table", comment: null },
  { name: "events", kind: "table", comment: null },
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
    rowId: null,
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
        rowId: null,
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
        rowId: null,
      };
    case "events":
      return {
        ...base,
        columns: [column("id", "INTEGER", false, 1), column("kind", "TEXT", false, 2)],
        primaryKey: { name: null, columns: ["id"] },
        rowKey: ["id"],
        rowId: null,
      };
    case "documents":
      return {
        ...base,
        columns: [
          column("id", "INTEGER", false, 1),
          { ...column("body", "JSON", true, 2), category: "json" },
        ],
        primaryKey: { name: null, columns: ["id"] },
        rowKey: ["id"],
        rowId: null,
      };
    default:
      return { ...base, kind: "view", columns: [column("name", "TEXT", true, 1)] };
  }
}

/** A JSON document longer than a grid cell shows; the page sends its start, truncated. */
export const MOCK_LONG_JSON = JSON.stringify({
  title: "Quarterly report",
  items: Array.from({ length: 40 }, (_, i) => ({ line: i, amount: i * 10 })),
});

const TABLE_ROWS: Record<string, Value[][]> = {
  people: peopleRows,
  orders: [
    [
      { t: "int", v: "1" },
      { t: "int", v: "1" },
    ],
    [
      { t: "int", v: "2" },
      { t: "int", v: "1" },
    ],
    [
      { t: "int", v: "3" },
      { t: "int", v: "2" },
    ],
  ],
  events: Array.from({ length: 450 }, (_, i): Value[] => [
    { t: "int", v: String(i + 1) },
    { t: "text", v: i % 3 === 0 ? "login" : "click" },
  ]),
  documents: [
    [
      { t: "int", v: "1" },
      { t: "json", v: MOCK_LONG_JSON.slice(0, 60), truncated: true },
    ],
    [
      { t: "int", v: "2" },
      { t: "json", v: '{"broken": ' },
    ],
  ],
};

const wireText = (value: Value | undefined) => (value && value.t !== "null" ? rawText(value) : "");

/** Enough of a WHERE clause for tests: `col = 'text'`, `col > number`; `FRM` is a syntax error. */
function filterRows(
  columns: string[],
  data: Value[][],
  filter: string | undefined,
): Value[][] | AppError {
  if (!filter) return data;
  if (/FRM|>>>/.test(filter))
    return { code: "syntax", message: `syntax error at or near "${filter}"` };
  const equals = /^(\w+)\s*=\s*'([^']*)'$/.exec(filter);
  const greater = /^(\w+)\s*>\s*(\d+)$/.exec(filter);
  const match = equals ?? greater;
  if (!match) return data;
  const index = columns.indexOf(match[1] ?? "");
  return data.filter((row) =>
    equals ? wireText(row[index]) === match[2] : Number(wireText(row[index])) > Number(match[2]),
  );
}

type Tables = Record<string, Value[][]>;

function tableRows(args: Record<string, unknown>, tables: Tables): Value[][] | AppError {
  const table = args.table as TableRef;
  const columns = mockTable(table).columns.map((c) => c.name);
  const all = tables[table.name] ?? [];
  const keyed = ((args.key as KeyArg[] | undefined) ?? []).reduce(
    (rows, k) =>
      rows.filter((row) => wireText(row[columns.indexOf(k.column)]) === wireText(k.value)),
    all,
  );
  return filterRows(columns, keyed, args.filter as string | undefined);
}

function page(args: Record<string, unknown>, tables: Tables): Promise<TablePage> {
  const table = args.table as TableRef;
  const info = mockTable(table);
  const found = tableRows(args, tables);
  if (!Array.isArray(found)) return Promise.reject(found);
  const names = info.columns.map((c) => c.name);
  const sorted = [...found];
  for (const key of [...((args.sort as SortKey[] | undefined) ?? [])].reverse()) {
    const i = names.indexOf(key.column);
    sorted.sort((a, b) => {
      const [x, y] = [wireText(a[i]), wireText(b[i])];
      const order =
        Number.isNaN(Number(x)) || Number.isNaN(Number(y))
          ? x.localeCompare(y)
          : Number(x) - Number(y);
      return key.descending ? -order : order;
    });
  }
  const offset = Number(args.offset);
  const limit = Number(args.limit);
  return Promise.resolve({
    columns: info.columns.map((c) => ({
      name: c.name,
      typeName: c.typeName,
      category: c.category,
      nullable: c.nullable,
    })),
    rows: sorted.slice(offset, offset + limit),
    hasMore: sorted.length > offset + limit,
    elapsedMs: 2,
  });
}

const quote = (name: string) => `"${name.replaceAll('"', '""')}"`;

/** The SSH server's fingerprint in the mock; a host named `changed.example.com` presents another. */
export const MOCK_FINGERPRINT = "SHA256:mockmockmockmockmockmockmockmockmockmockmock";
/** Running this statement makes the mock behave as if the SSH tunnel dropped. */
export const MOCK_DROP_TUNNEL = "SELECT drop_tunnel()";

/** A statement that runs until it is cancelled, like `SELECT pg_sleep(30)`. */
const SLEEP = /sleep\(/i;

const VIEW_STATEMENT_MS = 5;

export function createMockBackend() {
  let settings: Settings = { ...DEFAULT_SETTINGS };
  const profiles = new Map<string, ConnectionProfile>();
  /** Manual transaction mode per tab, mutated in place: this is the fake server. */
  const transactions = new Map<string, { manual: boolean; open: boolean }>();
  const sessions = new Map<string, ConnectionProfile>();
  /** Pending sleeps by tab, resolved by `query_cancel`. */
  const sleeping = new Map<string, () => void>();
  const knownHosts: KnownHost[] = [];
  const savedQueries: SavedQuery[] = [];
  /** Paths exports were written to. */
  const exports: string[] = [];
  /** Top-level parameters besides `args` of the current call (a progress channel). */
  let extras: Record<string, unknown> = {};
  /** This backend's own copy of the tables, so edits stay in one test. */
  let tables: Tables = structuredClone(TABLE_ROWS);

  /** Edits as the backend applies them: deletes, updates, inserts, all or nothing. */
  const applyChanges = (
    args: Args,
    preview: boolean,
  ): string[] | { statements: number } | AppError => {
    const table = args.table as TableRef;
    const info = mockTable(table);
    const session = sessions.get(args.sessionId as string);
    if (session?.readOnly) return { code: "read_only", message: "This connection is read-only" };
    if (info.kind !== "table" || !info.rowKey) {
      return {
        code: "invalid_argument",
        message: "The table has no primary key or unique not-null key, so it is read-only",
      };
    }
    const names = info.columns.map((c) => c.name);
    const operations = args.operations as OperationArg[];
    const order = (o: OperationArg) => ({ delete: 0, update: 1, insert: 2 })[o.kind];
    const planned = operations
      .map((operation, index) => ({ operation, index }))
      .sort((a, b) => order(a.operation) - order(b.operation));
    const literal = (v: Value) =>
      v.t === "null" ? "NULL" : `'${rawText(v).replaceAll("'", "''")}'`;
    const where = (key: KeyArg[]) =>
      key.map((k) => `${k.column} = ${literal(k.value)}`).join(" AND ");
    const sql = planned.map(({ operation: o }) =>
      o.kind === "delete"
        ? `DELETE FROM ${table.name} WHERE ${where(o.key)}`
        : o.kind === "update"
          ? `UPDATE ${table.name} SET ${o.set.map((s) => `${s.column} = ${literal(s.value)}`).join(", ")} WHERE ${where(o.key)}`
          : `INSERT INTO ${table.name} (${o.values.map((v) => v.column).join(", ")}) VALUES (${o.values.map((v) => literal(v.value)).join(", ")})`,
    );
    if (preview) return sql;
    const rows = structuredClone(tables[table.name] ?? []);
    const find = (key: KeyArg[]) =>
      rows.findIndex((row) =>
        key.every((k) => wireText(row[names.indexOf(k.column)]) === wireText(k.value)),
      );
    for (const { operation: o, index } of planned) {
      const fail = (code: string, message: string): AppError => ({
        code,
        message,
        detail: { operation: index },
      });
      if (o.kind === "insert") {
        const row: Value[] = names.map(
          (name) => o.values.find((v) => v.column === name)?.value ?? { t: "null" },
        );
        if (rows.some((r) => wireText(r[0]) === wireText(row[0])))
          return fail("constraint", "duplicate key value violates unique constraint");
        rows.push(row);
        continue;
      }
      const at = find(o.key);
      if (at < 0)
        return fail("row_count_mismatch", "Expected to change exactly one row, but 0 matched");
      if (o.kind === "delete") rows.splice(at, 1);
      else {
        const current = rows[at] ?? [];
        rows[at] = names.map(
          (name, i) => o.set.find((s) => s.column === name)?.value ?? current[i] ?? { t: "null" },
        );
      }
    }
    tables = { ...tables, [table.name]: rows };
    return { statements: planned.length };
  };
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
      folder: draft.folder?.trim() ? draft.folder.trim() : null,
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
        source: null,
      });
    }
    if (MOCK_WIDE_TABLE.test(sql)) return Promise.resolve(wideResult());
    if (MOCK_ROW_LIMIT_TABLE.test(sql)) {
      const data: Value[][] = Array.from({ length: args.rowLimit }, (_, i) => [
        { t: "int", v: String(i + 1) },
      ]);
      return Promise.resolve(rows([peopleColumns[0] as ColumnMeta], data, true));
    }
    if (/^select \* from people$/i.test(sql))
      return Promise.resolve(rows(peopleColumns, peopleRows, false, "people"));
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
    connection_duplicate: (a) => {
      const original = profiles.get(a.connectionId as string);
      if (!original) return reject("not_found", "No such connection");
      const copy = { ...original, id: `c${nextId++}`, name: `${original.name} copy` };
      const entries = [...profiles.entries()];
      const at = entries.findIndex(([id]) => id === original.id) + 1;
      entries.splice(at, 0, [copy.id, copy]);
      profiles.clear();
      for (const [id, profile] of entries) profiles.set(id, profile);
      return copy;
    },
    connection_reorder: (a) => {
      const order = a.order as string[];
      if (order.length !== profiles.size || !order.every((id) => profiles.has(id)))
        return reject("invalid_argument", "The new order must list every connection once");
      const reordered = order.map((id) => [id, profiles.get(id)] as [string, ConnectionProfile]);
      profiles.clear();
      for (const [id, profile] of reordered) profiles.set(id, profile);
      return [...profiles.values()];
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
    query_execute: (a) => {
      const args = a as unknown as QueryExecuteArgs;
      const state = transactions.get(args.tabId);
      const word = /^\s*(\w+)/.exec(args.sql)?.[1]?.toUpperCase() ?? "";
      if (state?.manual && ["BEGIN", "START"].includes(word)) state.open = true;
      else if (state && ["COMMIT", "ROLLBACK", "END"].includes(word)) state.open = false;
      else if (state?.manual) state.open = true;
      return record(args, execute(args));
    },
    connections_export: () => profiles.size,
    connections_import: () => {
      const imported = save({
        ...([...profiles.values()][0] ?? {}),
        id: null,
        name: "Imported",
        driver: "sqlite",
        filePath: MOCK_DATABASE_PATH,
      } as ConnectionDraft);
      return { imported: [imported], skipped: ["Broken: The host is required"] };
    },
    "plugin:dialog|save": (a) =>
      JSON.stringify(a).includes("dunlin-connections")
        ? "/mock/connections.json"
        : MOCK_NEW_DATABASE_PATH,
    ssh_config_hosts: () => [
      {
        alias: "bastion",
        hostName: "bastion.example.com",
        user: "ops",
        port: 2222,
        identityFile: null,
        proxyJump: null,
      },
      {
        alias: "db-prod",
        hostName: "db.internal",
        user: "admin",
        port: null,
        identityFile: "/home/me/.ssh/id_ed25519",
        proxyJump: "bastion",
      },
    ],
    query_explain: (a) => {
      const sql = a.sql as string;
      if (/FRM/.test(sql)) return reject("syntax", 'near "FRM": syntax error');
      return {
        operation: "Hash Join",
        relation: null,
        details: ["Hash Cond: (o.person = p.id)"],
        estimatedRows: 120,
        cost: 45.5,
        children: [
          {
            operation: "Seq Scan",
            relation: "people p",
            details: [],
            estimatedRows: 3,
            cost: 1.03,
            children: [],
          },
          {
            operation: "Index Scan",
            relation: "orders o",
            details: ["Index Name: orders_person"],
            estimatedRows: 40,
            cost: 30,
            children: [],
          },
        ],
      };
    },
    transaction_status: (a) => ({ ...(transactions.get(a.tabId as string) ?? NO_TRANSACTION) }),
    transaction_mode_set: (a) => {
      const current = transactions.get(a.tabId as string) ?? { ...NO_TRANSACTION };
      if (!a.manual && current.open)
        return reject("invalid_argument", "Commit or roll back the open transaction first");
      const next = { ...current, manual: a.manual as boolean };
      transactions.set(a.tabId as string, next);
      return { ...next };
    },
    transaction_finish: (a) => {
      const current = transactions.get(a.tabId as string) ?? { ...NO_TRANSACTION };
      const next = { ...current, open: false };
      transactions.set(a.tabId as string, next);
      return { ...next };
    },
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
    settings_get: () => ({ ...settings }),
    settings_set: (a) => {
      const next = a.settings as Settings;
      const { editorFontSize: font, defaultRowLimit: rows, defaultPageSize: page } = next;
      const inside = (n: number, b: { min: number; max: number }) => n >= b.min && n <= b.max;
      if (
        !inside(font, BOUNDS.editorFontSize) ||
        !inside(rows, BOUNDS.defaultRowLimit) ||
        !inside(page, BOUNDS.defaultPageSize) ||
        !inside(next.historyCap, BOUNDS.historyCap)
      )
        return reject("invalid_argument", "A setting is out of range");
      settings = { ...next };
      return { ...settings };
    },
    logs_dir: () => "/mock/logs",
    app_ready: () => 42,
    saved_query_list: () => [...savedQueries],
    saved_query_save: (a) => {
      const draft = a.query as SavedQueryDraft;
      if (!draft.name.trim()) return reject("invalid_argument", "The query needs a name");
      const saved: SavedQuery = {
        id: draft.id ?? `q${savedQueries.length + 1}`,
        connectionId: draft.connectionId ?? null,
        name: draft.name.trim(),
        sql: draft.sql,
        updatedAt: Date.now(),
      };
      const index = savedQueries.findIndex((q) => q.id === saved.id);
      if (index >= 0) savedQueries.splice(index, 1, saved);
      else savedQueries.push(saved);
      return saved;
    },
    saved_query_delete: (a) => {
      const index = savedQueries.findIndex((q) => q.id === a.id);
      if (index < 0) return reject("not_found", "The saved query was not found");
      savedQueries.splice(index, 1);
      return null;
    },
    export_text: (a) => {
      const format = a.format as { kind: string };
      const rows = a.rows as Value[][];
      if (format.kind === "json") return JSON.stringify(rows.map((r) => r.map(rawText)));
      return rows.map((r) => r.map(rawText).join(",")).join("\n");
    },
    export_result: (a) => {
      const rows = a.rows as Value[][];
      if (rows.some((r) => r.some((v) => "truncated" in v && v.truncated === true))) {
        return reject(
          "invalid_argument",
          "Some values were cut short for display; export the table to write them whole",
        );
      }
      exports.push(a.path as string);
      return { rowsWritten: String(rows.length), path: a.path };
    },
    export_table: (a) => {
      const found = tableRows(a, tables);
      if (!Array.isArray(found)) return Promise.reject(found);
      const channel = extras.onProgress as
        { onmessage?: (p: { rowsWritten: string }) => void } | undefined;
      if (typeof a.filter === "string" && SLEEP.test(a.filter)) {
        return new Promise((_resolve, rejectSleep) => {
          channel?.onmessage?.({ rowsWritten: "1000" });
          sleeping.set(a.viewId as string, () => {
            rejectSleep({ code: "cancelled", message: "canceling statement due to user request" });
          });
        });
      }
      channel?.onmessage?.({ rowsWritten: String(found.length) });
      exports.push(a.path as string);
      return { rowsWritten: String(found.length), path: a.path };
    },
    "plugin:opener|reveal_item_in_dir": () => null,
    changes_preview: (a) => {
      const result = applyChanges(a, true);
      return Array.isArray(result) ? result : Promise.reject(result);
    },
    changes_apply: (a) => {
      const result = applyChanges(a, false);
      if (Array.isArray(result) || !("statements" in result)) return Promise.reject(result);
      return { statements: result.statements, rowsAffected: String(result.statements) };
    },
    table_page: (a) => {
      if (typeof a.filter === "string" && SLEEP.test(a.filter)) {
        return new Promise((_resolve, rejectSleep) => {
          sleeping.set(a.viewId as string, () => {
            rejectSleep({ code: "cancelled", message: "canceling statement due to user request" });
          });
        });
      }
      return page(a, tables);
    },
    table_count: (a) => {
      const found = tableRows(a, tables);
      if (!Array.isArray(found)) return Promise.reject(found);
      const count = String(found.length);
      if (a.exact === true) return { kind: "exact", rows: count };
      return a.filter || (a.key as KeyArg[] | undefined)?.length
        ? { kind: "unknown" }
        : { kind: "estimate", rows: count };
    },
    cell_full_value: (a) => {
      const table = a.table as TableRef;
      const id = wireText((a.key as KeyArg[])[0]?.value);
      if (table.name === "documents" && id === "1") return { t: "json", v: MOCK_LONG_JSON };
      const names = mockTable(table).columns.map((c) => c.name);
      const row = (tables[table.name] ?? []).find((r) => wireText(r[0]) === id);
      return (
        row?.[names.indexOf(a.column as string)] ?? reject("not_found", "The row no longer exists")
      );
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
  };

  /** View connections with a statement in flight; a second one is refused, as by the backend. */
  const busyViews = new Set<string>();
  const oneAtATime = (cmd: string, args: Args, run: () => unknown): unknown => {
    const view = ["table_page", "table_count", "cell_full_value"].includes(cmd)
      ? (args.viewId as string)
      : null;
    if (!view) return run();
    if (busyViews.has(view))
      return reject("internal", "A statement is already running in this tab");
    busyViews.add(view);
    // A real statement takes a moment; without this, requests could never overlap here.
    return new Promise((resolve) => setTimeout(resolve, VIEW_STATEMENT_MS))
      .then(run)
      .finally(() => {
        busyViews.delete(view);
      });
  };

  return (cmd: string, payload?: unknown): unknown => {
    const handler = handlers[cmd];
    if (!handler) return reject("internal", `Mock backend has no command ${cmd}`);
    const envelope = (payload ?? {}) as { args?: Args; options?: Args } & Record<string, unknown>;
    extras = envelope;
    try {
      const args = envelope.args ?? envelope.options ?? {};
      return oneAtATime(cmd, args, () => handler(args));
    } catch (error) {
      return Promise.reject(error);
    }
  };
}

export function installMockBackend(): void {
  mockIPC(createMockBackend());
}
