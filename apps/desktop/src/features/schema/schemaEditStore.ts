/** Staged schema changes per table detail tab (task 9.11). The tab is unmounted while hidden,
 * so its changes live here; their count goes to the edit store, which the close, disconnect and
 * quit guards read (task 9.13). */
import { create } from "zustand";

import type { TableInfo } from "../../ipc";
import { useEdits } from "../edit/editStore";
import { EMPTY, type SchemaAction, type SchemaPending, changeCount, reduce } from "./schemaPending";

type SchemaEditState = {
  /** By workspace view key. */
  pending: Record<string, SchemaPending>;
  /** The session of each view with staged changes. */
  sessions: Record<string, string>;
  dispatch: (viewKey: string, sessionId: string, info: TableInfo, action: SchemaAction) => void;
  clear: (viewKey: string, sessionId: string) => void;
  dropSession: (sessionId: string) => void;
};

export const useSchemaEdits = create<SchemaEditState>()((set, get) => ({
  pending: {},
  sessions: {},

  dispatch: (viewKey, sessionId, info, action) => {
    const next = reduce(get().pending[viewKey] ?? EMPTY, action, info);
    set({
      pending: { ...get().pending, [viewKey]: next },
      sessions: { ...get().sessions, [viewKey]: sessionId },
    });
    useEdits.getState().setPending(viewKey, sessionId, changeCount(next));
  },

  clear: (viewKey, sessionId) => {
    const keep = <T>(record: Record<string, T>) =>
      Object.fromEntries(Object.entries(record).filter(([k]) => k !== viewKey));
    set({ pending: keep(get().pending), sessions: keep(get().sessions) });
    useEdits.getState().setPending(viewKey, sessionId, 0);
  },

  dropSession: (sessionId) => {
    for (const [viewKey, owner] of Object.entries(get().sessions)) {
      if (owner === sessionId) get().clear(viewKey, sessionId);
    }
  },
}));

export const pendingFor = (state: Pick<SchemaEditState, "pending">, viewKey: string) =>
  state.pending[viewKey] ?? EMPTY;
