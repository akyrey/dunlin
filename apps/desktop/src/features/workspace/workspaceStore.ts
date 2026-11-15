import { create } from "zustand";

import type { KeyArg, TableRef } from "../../ipc";

/** A tab in a session's main area: an SQL editor (keyed by its tab id) or a table's details. */
export type View =
  | { kind: "editor"; key: string }
  | { kind: "table"; key: string; table: TableRef }
  /** A table's rows; `match` limits them to one key (foreign key navigation). */
  | { kind: "data"; key: string; table: TableRef; match: KeyArg[] }
  /** A key of a key-value store (D-032): `id` is its base64 bytes, `name` its display text. */
  | { kind: "key"; key: string; id: string; name: string };

export type Workspace = { views: View[]; active: string | null };

type WorkspaceState = {
  /** By session id. */
  workspaces: Record<string, Workspace>;
  /** Adds an editor tab's view and focuses it. */
  openEditor: (sessionId: string, tabId: string) => void;
  /** Opens (or focuses) a table's detail tab. */
  openTable: (sessionId: string, table: TableRef) => void;
  /** Opens (or focuses) a table's rows, optionally only those matching `match`. */
  openData: (sessionId: string, table: TableRef, match?: KeyArg[]) => void;
  /** Opens (or focuses) a key of a key-value store. */
  openKey: (sessionId: string, id: string, name: string) => void;
  activate: (sessionId: string, key: string) => void;
  /** Focuses the next (`1`) or previous (`-1`) view, wrapping around. */
  cycle: (sessionId: string, step: 1 | -1) => void;
  /** Moves a view to `index` in the tab strip. */
  move: (sessionId: string, key: string, index: number) => void;
  close: (sessionId: string, key: string) => void;
  dropSession: (sessionId: string) => void;
};

export function tableKey(table: TableRef): string {
  return JSON.stringify([table.namespace.catalog, table.namespace.schema, table.name]);
}

const EMPTY: Workspace = { views: [], active: null };

export function workspaceOf(
  state: { workspaces: Record<string, Workspace> },
  sessionId: string,
): Workspace {
  return state.workspaces[sessionId] ?? EMPTY;
}

export const useWorkspace = create<WorkspaceState>()((set, get) => {
  const workspace = (sessionId: string) => workspaceOf(get(), sessionId);
  const put = (sessionId: string, next: Workspace) => {
    set({ workspaces: { ...get().workspaces, [sessionId]: next } });
  };
  const add = (sessionId: string, view: View) => {
    const current = workspace(sessionId);
    const views = current.views.some((v) => v.key === view.key)
      ? current.views
      : [...current.views, view];
    put(sessionId, { views, active: view.key });
  };

  return {
    workspaces: {},

    openEditor: (sessionId, tabId) => {
      add(sessionId, { kind: "editor", key: tabId });
    },

    openTable: (sessionId, table) => {
      add(sessionId, { kind: "table", key: tableKey(table), table });
    },

    openData: (sessionId, table, match = []) => {
      const key = `data:${tableKey(table)}:${JSON.stringify(match)}`;
      add(sessionId, { kind: "data", key, table, match });
    },

    openKey: (sessionId, id, name) => {
      add(sessionId, { kind: "key", key: `key:${id}`, id, name });
    },

    activate: (sessionId, key) => {
      const current = workspace(sessionId);
      if (current.views.some((v) => v.key === key)) put(sessionId, { ...current, active: key });
    },

    cycle: (sessionId, step) => {
      const current = workspace(sessionId);
      const count = current.views.length;
      if (count === 0) return;
      const index = current.views.findIndex((v) => v.key === current.active);
      const next = current.views[(index + step + count) % count];
      if (next) put(sessionId, { ...current, active: next.key });
    },

    move: (sessionId, key, index) => {
      const current = workspace(sessionId);
      const view = current.views.find((v) => v.key === key);
      if (!view) return;
      const rest = current.views.filter((v) => v.key !== key);
      const at = Math.max(0, Math.min(index, rest.length));
      put(sessionId, { ...current, views: [...rest.slice(0, at), view, ...rest.slice(at)] });
    },

    close: (sessionId, key) => {
      const current = workspace(sessionId);
      const index = current.views.findIndex((v) => v.key === key);
      if (index < 0) return;
      const views = current.views.filter((v) => v.key !== key);
      // Closing the active view focuses its neighbour.
      const neighbour = views[Math.min(index, views.length - 1)];
      const active = current.active === key ? (neighbour?.key ?? null) : current.active;
      put(sessionId, { views, active });
    },

    dropSession: (sessionId) => {
      set({
        workspaces: Object.fromEntries(
          Object.entries(get().workspaces).filter(([k]) => k !== sessionId),
        ),
      });
    },
  };
});
