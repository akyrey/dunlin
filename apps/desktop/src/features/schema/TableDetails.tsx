import { Skeleton } from "@roost/ui";
import { type ReactNode, useEffect, useRef, useState } from "react";

import {
  type AppError,
  type ForeignKey,
  type ReferentialAction,
  type TableInfo,
  type TableRef,
  schemaTable,
  schemaTableDdl,
  toAppError,
} from "../../ipc";
import { LazyReadOnlyCode as ReadOnlyCode } from "../editor/lazy";

type SubTab = "columns" | "indexes" | "constraints" | "ddl";

const SUB_TABS: { id: SubTab; label: string }[] = [
  { id: "columns", label: "Columns" },
  { id: "indexes", label: "Indexes" },
  { id: "constraints", label: "Constraints" },
  { id: "ddl", label: "DDL" },
];

type Loaded<T> =
  { status: "loading" } | { status: "loaded"; data: T } | { status: "failed"; error: AppError };

/** Runs `load` once, on mount; remount the component (via `key`) to load something else. */
function useLoaded<T>(load: () => Promise<T>): Loaded<T> {
  const [state, setState] = useState<Loaded<T>>({ status: "loading" });
  const first = useRef(load);
  useEffect(() => {
    let stale = false;
    first.current().then(
      (data) => {
        if (!stale) setState({ status: "loaded", data });
      },
      (caught: unknown) => {
        if (!stale) setState({ status: "failed", error: toAppError(caught) });
      },
    );
    return () => {
      stale = true;
    };
  }, []);
  return state;
}

type Props = {
  sessionId: string;
  table: TableRef;
  onOpenTable: (table: TableRef) => void;
};

/** A table's columns, indexes, constraints and DDL (task 4.9). Render it with a `key` per
 * session and table. */
export function TableDetails({ sessionId, table, onOpenTable }: Props) {
  const [tab, setTab] = useState<SubTab>("columns");
  const info = useLoaded(() => schemaTable(sessionId, table));
  const title = [table.namespace.schema, table.name].filter(Boolean).join(".");

  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-zinc-800 px-4 py-2">
        <h2 className="text-sm font-semibold">{title}</h2>
        {info.status === "loaded" && <Summary info={info.data} />}
      </header>
      <div
        role="tablist"
        aria-label="Table details"
        className="flex gap-1 border-b border-zinc-800 px-3"
      >
        {SUB_TABS.map(({ id, label }) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            className={`px-3 py-1.5 text-sm ${tab === id ? "border-b-2 border-sky-500 text-zinc-100" : "text-zinc-400 hover:text-zinc-200"}`}
            onClick={() => {
              setTab(id);
            }}
          >
            {label}
          </button>
        ))}
      </div>
      <div
        role="tabpanel"
        aria-label={SUB_TABS.find((t) => t.id === tab)?.label}
        className="min-h-0 flex-1 overflow-auto"
      >
        {tab === "ddl" ? (
          <Ddl sessionId={sessionId} table={table} />
        ) : (
          <LoadState load={info}>
            {(data) => {
              switch (tab) {
                case "columns":
                  return <Columns info={data} />;
                case "indexes":
                  return <Indexes info={data} />;
                default:
                  return <Constraints info={data} onOpenTable={onOpenTable} />;
              }
            }}
          </LoadState>
        )}
      </div>
    </div>
  );
}

/** Loaded only when its sub-tab is opened. */
function Ddl({ sessionId, table }: { sessionId: string; table: TableRef }) {
  const ddl = useLoaded(() => schemaTableDdl(sessionId, table));
  return (
    <LoadState load={ddl}>
      {(text) => <ReadOnlyCode text={text} label="Table DDL" language="sql" />}
    </LoadState>
  );
}

function LoadState<T>({ load, children }: { load: Loaded<T>; children: (data: T) => ReactNode }) {
  if (load.status === "loading") return <Skeleton rows={6} label="Loading the table" />;
  if (load.status === "failed") {
    return (
      <p role="alert" className="p-4 text-sm text-rose-300">
        {load.error.message}
      </p>
    );
  }
  return <>{children(load.data)}</>;
}

function Summary({ info }: { info: TableInfo }) {
  const kind = { table: "Table", view: "View", materialized_view: "Materialized view" }[info.kind];
  const editing = info.rowKey
    ? `rows addressed by ${info.rowKey.join(", ")}`
    : info.kind === "table"
      ? "read-only: no primary key or unique not-null key"
      : null;
  return (
    <p className="text-xs text-zinc-400">
      {kind}
      {editing && ` · ${editing}`}
      {info.comment && <span className="block text-zinc-500">{info.comment}</span>}
    </p>
  );
}

function Table({ head, rows, empty }: { head: string[]; rows: ReactNode[][]; empty: string }) {
  if (rows.length === 0) return <p className="p-4 text-sm text-zinc-500">{empty}</p>;
  return (
    <table className="w-full text-left text-sm">
      <thead className="sticky top-0 bg-zinc-900 text-xs text-zinc-400">
        <tr>
          {head.map((h) => (
            <th key={h} scope="col" className="px-3 py-1.5 font-medium">
              {h}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.map((cells, i) => (
          <tr key={i} className="border-t border-zinc-800/70">
            {cells.map((cell, j) => (
              <td key={j} className="px-3 py-1 align-top">
                {cell}
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const code = (text: string | null | undefined) =>
  text ? <code className="font-mono text-xs text-zinc-300">{text}</code> : null;

function Columns({ info }: { info: TableInfo }) {
  const keyColumns = new Set(info.primaryKey?.columns ?? []);
  return (
    <Table
      head={["#", "Name", "Type", "Nullable", "Default", "Comment"]}
      empty="No columns."
      rows={info.columns.map((c) => [
        c.ordinal,
        <span key="name">
          {c.name}
          {keyColumns.has(c.name) && <span className="ml-1 text-[10px] text-amber-300">PK</span>}
        </span>,
        code(c.typeName),
        c.nullable ? "yes" : "no",
        c.generated ? <span key="d">generated: {code(c.default)}</span> : code(c.default),
        c.comment,
      ])}
    />
  );
}

function Indexes({ info }: { info: TableInfo }) {
  return (
    <Table
      head={["Name", "Columns", "Unique", "Method", "Where"]}
      empty="No indexes besides the keys."
      rows={info.indexes.map((i) => [
        i.name,
        code(i.columns.join(", ")),
        i.unique ? "yes" : "no",
        i.method,
        code(i.predicate),
      ])}
    />
  );
}

const ACTION_SQL: Record<ReferentialAction, string> = {
  cascade: "CASCADE",
  set_null: "SET NULL",
  set_default: "SET DEFAULT",
  restrict: "RESTRICT",
};

const action = (value: ReferentialAction | null) => (value ? ACTION_SQL[value] : "NO ACTION");

function Constraints({
  info,
  onOpenTable,
}: {
  info: TableInfo;
  onOpenTable: (table: TableRef) => void;
}) {
  const keys = [
    ...(info.primaryKey
      ? [["Primary key", info.primaryKey.name, code(info.primaryKey.columns.join(", "))]]
      : []),
    ...info.uniqueConstraints.map((k) => ["Unique", k.name, code(k.columns.join(", "))]),
    ...info.checkConstraints.map((c) => ["Check", c.name, code(c.expression)]),
  ];
  return (
    <div className="flex flex-col gap-4 py-2">
      <section aria-label="Keys and checks">
        <Table head={["Kind", "Name", "Definition"]} rows={keys} empty="No keys or checks." />
      </section>
      <section aria-label="Foreign keys">
        <Table
          head={["Name", "Columns", "References", "On delete", "On update"]}
          empty="No foreign keys."
          rows={info.foreignKeys.map((fk) => [
            fk.name,
            code(fk.columns.join(", ")),
            <Reference key="ref" fk={fk} onOpenTable={onOpenTable} />,
            action(fk.onDelete),
            action(fk.onUpdate),
          ])}
        />
      </section>
    </div>
  );
}

function Reference({
  fk,
  onOpenTable,
}: {
  fk: ForeignKey;
  onOpenTable: (table: TableRef) => void;
}) {
  const target = fk.referencedTable;
  const name = [target.namespace.schema, target.name].filter(Boolean).join(".");
  return (
    <button
      type="button"
      className="text-sky-400 hover:underline"
      onClick={() => {
        onOpenTable(target);
      }}
    >
      {name}
      <span className="text-zinc-400"> ({fk.referencedColumns.join(", ")})</span>
    </button>
  );
}
