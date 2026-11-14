import { create } from "zustand";

import {
  type AppError,
  type QueryResult,
  queryCancel,
  queryExecute,
  sqlSplit,
  toAppError,
} from "../../ipc";
import { statementAt } from "../../lib/text";
import { useSchema } from "../schema/schemaStore";
import { useSessions } from "../sessions/sessionsStore";

export const DEFAULT_ROW_LIMIT = 1000;

export type QueryError = {
  error: AppError;
  /** Absolute UTF-16 offset in the editor text, when the server reported a position. */
  offset: number | null;
};

export type PendingConfirmation = { reason: string; sql: string; start: number };

export type TabState = {
  tabId: string;
  text: string;
  /** Bumped when the text is replaced from outside the editor, which remounts it. */
  revision: number;
  running: boolean;
  /** `Date.now()` when the running statement started, for the elapsed timer. */
  startedAt: number | null;
  result: QueryResult | null;
  error: QueryError | null;
  confirmation: PendingConfirmation | null;
};

export type Selection = { from: number; to: number; head: number };

type EditorState = {
  /** One editor tab per session in Phase 1, keyed by session id. */
  tabs: Record<string, TabState>;
  ensureTab: (sessionId: string) => void;
  setText: (sessionId: string, text: string) => void;
  /** Adds text after what the editor holds (e.g. a generated SELECT). */
  insertText: (sessionId: string, text: string) => void;
  run: (sessionId: string, selection: Selection) => Promise<void>;
  confirm: (sessionId: string) => Promise<void>;
  cancel: (sessionId: string) => Promise<void>;
  dismissConfirmation: (sessionId: string) => void;
  dropTab: (sessionId: string) => void;
};

function newTab(): TabState {
  return {
    tabId: crypto.randomUUID(),
    text: "",
    revision: 0,
    running: false,
    startedAt: null,
    result: null,
    error: null,
    confirmation: null,
  };
}

function positionOf(error: AppError): number | null {
  const detail = error.detail;
  if (typeof detail !== "object" || detail === null) return null;
  const position = (detail as Record<string, unknown>).position;
  return typeof position === "number" ? position : null;
}

function noStatement(): QueryError {
  return {
    error: { code: "invalid_argument", message: "There is no statement to run" },
    offset: null,
  };
}

export const useEditor = create<EditorState>()((set, get) => {
  const update = (sessionId: string, patch: Partial<TabState>) => {
    const tab = get().tabs[sessionId];
    if (!tab) return;
    set({ tabs: { ...get().tabs, [sessionId]: { ...tab, ...patch } } });
  };

  const execute = async (sessionId: string, sql: string, start: number, confirmed: boolean) => {
    const tab = get().tabs[sessionId];
    if (!tab) return;
    update(sessionId, { running: true, startedAt: Date.now(), error: null, confirmation: null });
    try {
      const result = await queryExecute({
        sessionId,
        tabId: tab.tabId,
        sql,
        rowLimit: DEFAULT_ROW_LIMIT,
        confirmed,
      });
      update(sessionId, { running: false, startedAt: null, result });
      // The backend cleared its schema cache; show the change in the tree.
      if (result.statementClass === "ddl") void useSchema.getState().reload(sessionId);
    } catch (caught) {
      const error = toAppError(caught);
      if (error.code === "tunnel_lost") {
        // The backend already closed the session; nothing in this tab can run any more.
        set({
          tabs: Object.fromEntries(Object.entries(get().tabs).filter(([k]) => k !== sessionId)),
        });
        useSessions.getState().markLost(sessionId, error.message);
        return;
      }
      if (error.code === "confirmation_required") {
        update(sessionId, {
          running: false,
          startedAt: null,
          confirmation: { reason: error.message, sql, start },
        });
        return;
      }
      const position = positionOf(error);
      update(sessionId, {
        running: false,
        startedAt: null,
        result: null,
        error: { error, offset: position === null ? null : start + position },
      });
    }
  };

  return {
    tabs: {},

    ensureTab: (sessionId) => {
      if (get().tabs[sessionId]) return;
      set({ tabs: { ...get().tabs, [sessionId]: newTab() } });
    },

    setText: (sessionId, text) => {
      update(sessionId, { text });
    },

    insertText: (sessionId, text) => {
      get().ensureTab(sessionId);
      const tab = get().tabs[sessionId];
      if (!tab) return;
      const joined = tab.text.trim() === "" ? text : `${tab.text.trimEnd()}\n\n${text}`;
      update(sessionId, { text: joined, revision: tab.revision + 1 });
    },

    run: async (sessionId, selection) => {
      const tab = get().tabs[sessionId];
      if (!tab || tab.running) return;
      if (selection.from !== selection.to) {
        const sql = tab.text.slice(selection.from, selection.to);
        if (sql.trim() === "") {
          update(sessionId, { error: noStatement() });
          return;
        }
        await execute(sessionId, sql, selection.from, false);
        return;
      }
      try {
        const range = statementAt(await sqlSplit(sessionId, tab.text), selection.head);
        if (!range) {
          update(sessionId, { error: noStatement() });
          return;
        }
        await execute(sessionId, tab.text.slice(range.start, range.end), range.start, false);
      } catch (caught) {
        update(sessionId, { error: { error: toAppError(caught), offset: null } });
      }
    },

    confirm: async (sessionId) => {
      const pending = get().tabs[sessionId]?.confirmation;
      if (!pending) return;
      await execute(sessionId, pending.sql, pending.start, true);
    },

    cancel: async (sessionId) => {
      const tab = get().tabs[sessionId];
      if (!tab?.running) return;
      try {
        await queryCancel(sessionId, tab.tabId);
      } catch (caught) {
        update(sessionId, { error: { error: toAppError(caught), offset: null } });
      }
    },

    dismissConfirmation: (sessionId) => {
      update(sessionId, { confirmation: null });
    },

    dropTab: (sessionId) => {
      set({
        tabs: Object.fromEntries(Object.entries(get().tabs).filter(([k]) => k !== sessionId)),
      });
    },
  };
});
