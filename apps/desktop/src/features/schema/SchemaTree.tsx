import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";

import { type AppError, toAppError } from "../../ipc";
import { type ObjectAction, OBJECT_ACTIONS, runObjectAction } from "./actions";
import { SESSION_KEY, useSchema } from "./schemaStore";
import { type TreeRow, visibleRows } from "./tree";

const ROW_HEIGHT = 24;
const INDENT = 12;
const OVERSCAN = 20;

export type TreeSessionInfo = { connectionId: string; sessionId: string; label: string };

type ObjectRow = Extract<TreeRow, { kind: "object" }>;
type Menu = { row: ObjectRow; connectionId: string; x: number; y: number };

const KIND_LABEL = { table: "table", view: "view", materialized_view: "materialized view" };

/** The schema browser: every open session, lazily loaded and virtualised (task 4.8). */
export function SchemaTree({ sessions }: { sessions: TreeSessionInfo[] }) {
  const { trees, filter, showSystem, setFilter, setShowSystem, toggle, refresh } = useSchema();
  const [menu, setMenu] = useState<Menu | null>(null);
  const [actionError, setActionError] = useState<AppError | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // A newly opened session starts expanded.
  useEffect(() => {
    for (const { sessionId } of sessions) {
      if (!useSchema.getState().trees[sessionId]) void toggle(sessionId, SESSION_KEY);
    }
  }, [sessions, toggle]);

  const rows = useMemo(
    () =>
      visibleRows(
        sessions.flatMap(({ sessionId, label }) => {
          const tree = trees[sessionId];
          return tree ? [{ sessionId, label, tree }] : [];
        }),
        { filter, showSystem },
      ),
    [sessions, trees, filter, showSystem],
  );
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: OVERSCAN,
  });
  const connectionOf = (sessionId: string) =>
    sessions.find((s) => s.sessionId === sessionId)?.connectionId ?? "";

  const act = async (action: ObjectAction, row: ObjectRow) => {
    setMenu(null);
    setActionError(null);
    try {
      await runObjectAction(action, {
        connectionId: connectionOf(row.sessionId),
        sessionId: row.sessionId,
        table: { namespace: row.namespace, name: row.object.name },
      });
    } catch (caught) {
      setActionError(toAppError(caught));
    }
  };

  if (sessions.length === 0) return null;
  return (
    <section aria-label="Schema" className="flex min-h-0 flex-1 flex-col border-t border-zinc-800">
      <div className="flex items-center gap-2 px-3 py-1.5">
        <input
          className="input min-w-0 flex-1 py-0.5 text-xs"
          placeholder="Filter tables"
          aria-label="Filter tables"
          value={filter}
          onChange={(e) => {
            setFilter(e.target.value);
          }}
        />
        <button
          type="button"
          className="text-xs text-zinc-400 hover:text-zinc-100"
          onClick={() => {
            for (const { sessionId } of sessions) void refresh(sessionId);
          }}
        >
          Refresh
        </button>
      </div>
      <label className="flex items-center gap-2 px-3 pb-1 text-xs text-zinc-400">
        <input
          type="checkbox"
          checked={showSystem}
          onChange={(e) => {
            setShowSystem(e.target.checked);
          }}
        />
        Show system objects
      </label>
      {actionError && (
        <p role="alert" className="mx-3 mb-1 text-xs text-rose-300">
          {actionError.message}
        </p>
      )}
      <div
        ref={scrollRef}
        role="tree"
        aria-label="Schema tree"
        className="min-h-0 flex-1 overflow-auto"
      >
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((item) => {
            const row = rows[item.index];
            if (!row) return null;
            return (
              <div
                key={row.key}
                className="absolute left-0 right-0"
                style={{ top: item.start, height: ROW_HEIGHT, paddingLeft: 8 + row.depth * INDENT }}
              >
                <Row
                  row={row}
                  onToggle={(r) =>
                    void toggle(
                      r.sessionId,
                      nodeKey(r),
                      r.kind === "namespace" ? r.namespace : undefined,
                    )
                  }
                  onAction={(action, r) => void act(action, r)}
                  onMenu={(r, x, y) => {
                    setMenu({ row: r, connectionId: connectionOf(r.sessionId), x, y });
                  }}
                />
              </div>
            );
          })}
        </div>
      </div>
      {menu && (
        <ObjectMenu
          menu={menu}
          onAction={(action) => void act(action, menu.row)}
          onClose={() => {
            setMenu(null);
          }}
        />
      )}
    </section>
  );
}

/** The store's node key: row keys are `<sessionId>/<node key>`. */
function nodeKey(row: TreeRow): string {
  return row.key.slice(row.sessionId.length + 1);
}

type RowProps = {
  row: TreeRow;
  onToggle: (row: TreeRow) => void;
  onAction: (action: ObjectAction, row: ObjectRow) => void;
  onMenu: (row: ObjectRow, x: number, y: number) => void;
};

function Row({ row, onToggle, onAction, onMenu }: RowProps) {
  const level = row.depth + 1;
  switch (row.kind) {
    case "session":
    case "namespace":
    case "group":
      return (
        <div role="treeitem" aria-level={level} aria-expanded={row.expanded} aria-selected={false}>
          <button
            type="button"
            className="flex h-6 w-full items-center gap-1 truncate pr-2 text-left text-sm hover:bg-zinc-800/60"
            onClick={() => {
              onToggle(row);
            }}
          >
            <span aria-hidden="true" className="w-3 text-zinc-500">
              {row.expanded ? "▾" : "▸"}
            </span>
            <span className={row.kind === "group" ? "text-xs text-zinc-400" : "truncate"}>
              {row.label}
            </span>
            {row.kind === "group" && <span className="text-xs text-zinc-600">{row.count}</span>}
          </button>
        </div>
      );
    case "object":
      return (
        <div
          role="treeitem"
          aria-level={level}
          aria-selected={false}
          className="group flex h-6 items-center pr-1 hover:bg-zinc-800/60"
          onContextMenu={(e) => {
            e.preventDefault();
            onMenu(row, e.clientX, e.clientY);
          }}
        >
          <button
            type="button"
            className="flex-1 truncate text-left text-sm"
            title={row.object.comment ?? `${row.object.name} (${KIND_LABEL[row.object.kind]})`}
            onClick={() => {
              onAction("data", row);
            }}
          >
            {row.object.name}
          </button>
          <button
            type="button"
            aria-label={`Actions for ${row.object.name}`}
            className="px-1 text-xs text-zinc-500 opacity-0 hover:text-zinc-100 focus:opacity-100 group-hover:opacity-100"
            onClick={(e) => {
              const rect = e.currentTarget.getBoundingClientRect();
              onMenu(row, rect.left, rect.bottom);
            }}
          >
            ⋯
          </button>
        </div>
      );
    case "loading":
      return (
        <p className="flex h-6 items-center">
          <span aria-hidden="true" className="h-2.5 w-28 animate-pulse rounded bg-zinc-800" />
          <span className="sr-only">Loading…</span>
        </p>
      );
    case "error":
      return (
        <p
          role="alert"
          className="h-6 truncate text-xs leading-6 text-rose-300"
          title={row.error.message}
        >
          {row.error.message}
        </p>
      );
    case "empty":
      return <p className="h-6 text-xs leading-6 text-zinc-500">{row.label}</p>;
  }
}

function ObjectMenu({
  menu,
  onAction,
  onClose,
}: {
  menu: Menu;
  onAction: (action: ObjectAction) => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    ref.current?.querySelector("button")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    const onPointer = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onPointer);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onPointer);
    };
  }, [onClose]);

  return (
    <div
      ref={ref}
      role="menu"
      aria-label={`Actions for ${menu.row.object.name}`}
      className="fixed z-50 flex min-w-44 flex-col rounded border border-zinc-700 bg-zinc-900 py-1 shadow-lg"
      style={{ left: menu.x, top: menu.y }}
    >
      {OBJECT_ACTIONS.map(({ action, label }) => (
        <button
          key={action}
          type="button"
          role="menuitem"
          className="px-3 py-1 text-left text-sm hover:bg-zinc-800 focus:bg-zinc-800 focus:outline-none"
          onClick={() => {
            onAction(action);
          }}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
