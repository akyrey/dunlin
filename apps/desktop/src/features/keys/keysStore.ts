import { create } from "zustand";

import { type AppError, type KeySummary, kvScan, toAppError } from "../../ipc";

/** Keys per step; the server may return more or fewer. */
export const SCAN_COUNT = 200;
/** Steps taken per "load" when steps come back empty (sparse matches in a big keyspace). */
const MAX_EMPTY_STEPS = 20;

export type KeyList = {
  pattern: string;
  keys: KeySummary[];
  /** Where the next step starts; `null` when the scan is complete. */
  cursor: string | null;
  loading: boolean;
  error: AppError | null;
};

const FRESH: KeyList = { pattern: "*", keys: [], cursor: null, loading: false, error: null };

type KeysState = {
  /** By session id. */
  lists: Record<string, KeyList>;
  /** Starts a new scan with `pattern`. */
  search: (sessionId: string, pattern: string) => Promise<void>;
  /** Continues the scan. */
  more: (sessionId: string) => Promise<void>;
};

export function listOf(state: Pick<KeysState, "lists">, sessionId: string): KeyList {
  return state.lists[sessionId] ?? FRESH;
}

/** The key browser's state (D-032): SCAN steps, never KEYS. */
export const useKeys = create<KeysState>()((set, get) => {
  const put = (sessionId: string, patch: Partial<KeyList>) => {
    set({ lists: { ...get().lists, [sessionId]: { ...listOf(get(), sessionId), ...patch } } });
  };

  /** Steps until some keys arrive or the scan ends, so an empty step is not a dead end. */
  const step = async (
    sessionId: string,
    pattern: string,
    from: string | null,
    known: KeySummary[],
  ) => {
    put(sessionId, { loading: true, error: null });
    try {
      let cursor = from;
      let found: KeySummary[] = [];
      for (let i = 0; i < MAX_EMPTY_STEPS; i++) {
        const page = await kvScan(sessionId, pattern, cursor ?? undefined, SCAN_COUNT);
        found = [...found, ...page.keys];
        cursor = page.cursor;
        if (found.length > 0 || cursor === null) break;
      }
      // A stale answer for an older pattern is dropped.
      if (listOf(get(), sessionId).pattern !== pattern) return;
      const seen = new Set(known.map((k) => k.key.id));
      put(sessionId, {
        keys: [...known, ...found.filter((k) => !seen.has(k.key.id))],
        cursor,
        loading: false,
      });
    } catch (caught) {
      put(sessionId, { loading: false, error: toAppError(caught) });
    }
  };

  return {
    lists: {},
    search: async (sessionId, pattern) => {
      const clean = pattern.trim() || "*";
      put(sessionId, { ...FRESH, pattern: clean });
      await step(sessionId, clean, null, []);
    },
    more: async (sessionId) => {
      const list = listOf(get(), sessionId);
      if (list.loading || list.cursor === null) return;
      await step(sessionId, list.pattern, list.cursor, list.keys);
    },
  };
});
