import { create } from "zustand";

import {
  type AppError,
  type SavedQuery,
  type SavedQueryDraft,
  savedQueryDelete,
  savedQueryList,
  savedQuerySave,
  toAppError,
} from "../../ipc";

type SavedQueriesState = {
  queries: SavedQuery[];
  error: AppError | null;
  load: () => Promise<void>;
  /** Creates or updates a query; resolves with what was stored. */
  save: (draft: SavedQueryDraft) => Promise<SavedQuery>;
  remove: (id: string) => Promise<void>;
};

const byName = (a: SavedQuery, b: SavedQuery) =>
  a.name.localeCompare(b.name, undefined, { sensitivity: "base" });

export const useSavedQueries = create<SavedQueriesState>()((set, get) => ({
  queries: [],
  error: null,

  load: async () => {
    try {
      set({ queries: await savedQueryList(), error: null });
    } catch (caught) {
      set({ error: toAppError(caught) });
    }
  },

  save: async (draft) => {
    const saved = await savedQuerySave(draft);
    set({ queries: [...get().queries.filter((q) => q.id !== saved.id), saved].sort(byName) });
    return saved;
  },

  remove: async (id) => {
    await savedQueryDelete(id);
    set({ queries: get().queries.filter((q) => q.id !== id) });
  },
}));

/** Queries shown for a connection: its own, then those for every connection. */
export function queriesFor(queries: SavedQuery[], connectionId: string | null): SavedQuery[] {
  return [
    ...queries.filter((q) => connectionId !== null && q.connectionId === connectionId),
    ...queries.filter((q) => q.connectionId === null),
  ];
}
