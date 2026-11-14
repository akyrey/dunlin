import { create } from "zustand";

import type { TableRef } from "../../ipc";

/** The SQL editor's view key; every other view is a table detail tab. */
export const EDITOR_VIEW = "editor";

export type DetailTab = { key: string; table: TableRef };

export type Workspace = { details: DetailTab[]; active: string };

type WorkspaceState = {
  /** By session id. */
  workspaces: Record<string, Workspace>;
  /** Opens (or focuses) a table's detail tab. */
  openTable: (sessionId: string, table: TableRef) => void;
  activate: (sessionId: string, key: string) => void;
  close: (sessionId: string, key: string) => void;
  dropSession: (sessionId: string) => void;
};

export function tableKey(table: TableRef): string {
  return JSON.stringify([table.namespace.catalog, table.namespace.schema, table.name]);
}

const EMPTY: Workspace = { details: [], active: EDITOR_VIEW };

export const useWorkspace = create<WorkspaceState>()((set, get) => {
  const workspace = (sessionId: string) => get().workspaces[sessionId] ?? EMPTY;
  const put = (sessionId: string, next: Workspace) => {
    set({ workspaces: { ...get().workspaces, [sessionId]: next } });
  };

  return {
    workspaces: {},

    openTable: (sessionId, table) => {
      const current = workspace(sessionId);
      const key = tableKey(table);
      const details = current.details.some((d) => d.key === key)
        ? current.details
        : [...current.details, { key, table }];
      put(sessionId, { details, active: key });
    },

    activate: (sessionId, key) => {
      put(sessionId, { ...workspace(sessionId), active: key });
    },

    close: (sessionId, key) => {
      const current = workspace(sessionId);
      const index = current.details.findIndex((d) => d.key === key);
      if (index < 0) return;
      const details = current.details.filter((d) => d.key !== key);
      // Closing the active tab focuses its neighbour, or the editor.
      const neighbour = details[Math.min(index, details.length - 1)];
      const active = current.active === key ? (neighbour?.key ?? EDITOR_VIEW) : current.active;
      put(sessionId, { details, active });
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

export function workspaceOf(state: WorkspaceState, sessionId: string): Workspace {
  return state.workspaces[sessionId] ?? EMPTY;
}
