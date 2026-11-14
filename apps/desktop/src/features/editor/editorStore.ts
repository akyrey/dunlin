import { create } from "zustand";

import {
  type AppError,
  type QueryResult,
  type StatementRange,
  queryCancel,
  queryExecute,
  sqlSplit,
  tabClose,
  toAppError,
} from "../../ipc";
import { useSchema } from "../schema/schemaStore";
import { useSessions } from "../sessions/sessionsStore";
import { useWorkspace } from "../workspace/workspaceStore";
import { useCompletion } from "./completionStore";
import {
  type Confirmation,
  type Run,
  type RunMode,
  type Selection,
  type StatementRun,
  describeOutcome,
  plan,
  positionOf,
} from "./run";

export const DEFAULT_ROW_LIMIT = 1000;
export const MAX_ROW_LIMIT = 100_000;

export type EditorTab = {
  tabId: string;
  sessionId: string;
  title: string;
  text: string;
  /** Bumped when the text is replaced from outside the editor, which remounts it. */
  revision: number;
  rowLimit: number;
  running: boolean;
  /** `Date.now()` when the running statement started, for the elapsed timer. */
  startedAt: number | null;
  run: Run | null;
  /** A problem before anything ran (nothing to run, split failed). */
  notice: AppError | null;
  confirmation: Confirmation | null;
  /** The result sub-tab shown: a statement index, or the Messages log. */
  shown: number | "messages";
  /** `sql_split` of `text`, cached per document (task 5.5). */
  split: { text: string; ranges: StatementRange[] } | null;
};

export type NewTab = { title?: string; text?: string; rowLimit?: number };

type EditorState = {
  tabs: Record<string, EditorTab>;
  /** Opens a new editor tab on the session and focuses it. Returns its id. */
  openTab: (sessionId: string, init?: NewTab) => string;
  setText: (tabId: string, text: string) => void;
  /** Adds text after what the tab holds (e.g. a statement from history). */
  insertText: (tabId: string, text: string) => void;
  rename: (tabId: string, title: string) => void;
  setRowLimit: (tabId: string, rowLimit: number) => void;
  show: (tabId: string, shown: number | "messages") => void;
  /** Splits the tab's text unless the cached split is for the same text. */
  updateSplit: (tabId: string) => Promise<StatementRange[]>;
  run: (tabId: string, selection: Selection, mode: RunMode) => Promise<void>;
  /** Runs the statements waiting in the confirmation, with the dangerous ones confirmed. */
  confirm: (tabId: string) => Promise<void>;
  dismissConfirmation: (tabId: string) => void;
  cancel: (tabId: string) => Promise<void>;
  /** Closes the tab and its backend connection. */
  closeTab: (tabId: string) => Promise<void>;
  /** Forgets every tab of a session the backend already closed. */
  dropSession: (sessionId: string) => void;
};

function nextTitle(tabs: EditorTab[]): string {
  const numbers = tabs
    .map((t) => /^Query (\d+)$/.exec(t.title)?.[1])
    .map(Number)
    .filter(Boolean);
  return `Query ${Math.max(0, ...numbers) + 1}`;
}

function noStatement(): AppError {
  return { code: "invalid_argument", message: "There is no statement to run" };
}

/** The result the tab's results panel shows, for the status bar. */
export function shownResult(tab: EditorTab | undefined): QueryResult | null {
  if (!tab?.run || tab.shown === "messages") return null;
  return tab.run.statements[tab.shown]?.result ?? null;
}

export function tabsOf(state: { tabs: Record<string, EditorTab> }, sessionId: string): EditorTab[] {
  return Object.values(state.tabs).filter((t) => t.sessionId === sessionId);
}

export const useEditor = create<EditorState>()((set, get) => {
  const update = (
    tabId: string,
    patch: Partial<EditorTab> | ((tab: EditorTab) => Partial<EditorTab>),
  ) => {
    const tab = get().tabs[tabId];
    if (!tab) return;
    const change = typeof patch === "function" ? patch(tab) : patch;
    set({ tabs: { ...get().tabs, [tabId]: { ...tab, ...change } } });
  };
  const updateRun = (tabId: string, change: (run: Run) => Run) => {
    update(tabId, (tab) => (tab.run ? { run: change(tab.run) } : {}));
  };
  const setStatement = (tabId: string, index: number, patch: Partial<StatementRun>) => {
    updateRun(tabId, (run) => ({
      ...run,
      statements: run.statements.map((s, i) => (i === index ? { ...s, ...patch } : s)),
    }));
  };
  const log = (tabId: string, level: "info" | "error", text: string) => {
    updateRun(tabId, (run) => ({
      ...run,
      messages: [...run.messages, { at: Date.now(), level, text }],
    }));
  };
  const skipFrom = (tabId: string, index: number, status: "skipped" | "cancelled") => {
    updateRun(tabId, (run) => ({
      ...run,
      statements: run.statements.map((s, i) =>
        i >= index && s.status === "pending" ? { ...s, status } : s,
      ),
    }));
  };
  const dropSessionTabs = (sessionId: string) => {
    set({
      tabs: Object.fromEntries(
        Object.entries(get().tabs).filter(([, t]) => t.sessionId !== sessionId),
      ),
    });
  };

  /** Runs one statement; returns false when the run must stop. */
  const runOne = async (tabId: string, index: number, confirmed: boolean): Promise<boolean> => {
    const tab = get().tabs[tabId];
    const statement = tab?.run?.statements[index];
    if (!tab || !statement) return false;
    if (tab.run?.cancelRequested) {
      skipFrom(tabId, index, "cancelled");
      log(tabId, "info", "Cancelled before the remaining statements ran");
      return false;
    }
    setStatement(tabId, index, { status: "running" });
    update(tabId, { startedAt: Date.now() });
    try {
      const result = await queryExecute({
        sessionId: tab.sessionId,
        tabId,
        sql: statement.sql,
        rowLimit: tab.rowLimit,
        confirmed: confirmed && statement.danger !== null ? true : undefined,
      });
      setStatement(tabId, index, { status: "done", result });
      log(tabId, "info", describeOutcome(index, result));
      if (result.statementClass === "ddl") {
        // The backend cleared its schema cache; show the change in the tree and autocomplete.
        void useSchema.getState().reload(tab.sessionId);
        void useCompletion.getState().reload(tab.sessionId);
      }
      return true;
    } catch (caught) {
      return failed(tabId, index, toAppError(caught));
    }
  };

  const failed = (tabId: string, index: number, error: AppError): boolean => {
    const tab = get().tabs[tabId];
    const statement = tab?.run?.statements[index];
    if (!tab || !statement) return false;
    if (error.code === "tunnel_lost") {
      // The backend closed the session; nothing in its tabs can run any more.
      useSessions.getState().markLost(tab.sessionId, error.message);
      dropSessionTabs(tab.sessionId);
      return false;
    }
    if (error.code === "confirmation_required") {
      // The backend found a danger the split did not report: ask, then resume here.
      const rest = (tab.run?.statements ?? [])
        .slice(index)
        .map((s, i) =>
          i === 0 ? { ...s, status: "pending" as const, danger: s.danger ?? error.message } : s,
        );
      setStatement(tabId, index, { status: "pending" });
      skipFrom(tabId, index + 1, "skipped");
      update(tabId, { confirmation: { statements: rest } });
      return false;
    }
    const position = positionOf(error);
    const status = error.code === "cancelled" ? "cancelled" : "failed";
    setStatement(tabId, index, {
      status,
      error,
      errorOffset: position === null ? null : statement.start + position,
    });
    log(tabId, "error", `Statement ${index + 1}: ${error.message}`);
    skipFrom(tabId, index + 1, status === "cancelled" ? "cancelled" : "skipped");
    return false;
  };

  /** Runs `statements` in order, stopping at the first error or a cancel. */
  const execute = async (tabId: string, statements: StatementRun[], confirmed: boolean) => {
    if (!confirmed && statements.some((s) => s.danger !== null)) {
      update(tabId, { confirmation: { statements } });
      return;
    }
    const plural = statements.length === 1 ? "statement" : "statements";
    update(tabId, {
      running: true,
      notice: null,
      confirmation: null,
      shown: 0,
      run: {
        statements,
        cancelRequested: false,
        messages: [
          { at: Date.now(), level: "info", text: `Running ${statements.length} ${plural}` },
        ],
      },
    });
    let last = 0;
    for (let index = 0; index < statements.length; index++) {
      last = index;
      if (!(await runOne(tabId, index, confirmed))) break;
    }
    update(tabId, { running: false, startedAt: null, shown: last });
  };

  return {
    tabs: {},

    openTab: (sessionId, init = {}) => {
      const tabId = crypto.randomUUID();
      const tab: EditorTab = {
        tabId,
        sessionId,
        title: init.title ?? nextTitle(tabsOf(get(), sessionId)),
        text: init.text ?? "",
        revision: 0,
        rowLimit: init.rowLimit ?? DEFAULT_ROW_LIMIT,
        running: false,
        startedAt: null,
        run: null,
        notice: null,
        confirmation: null,
        shown: 0,
        split: null,
      };
      set({ tabs: { ...get().tabs, [tabId]: tab } });
      useWorkspace.getState().openEditor(sessionId, tabId);
      return tabId;
    },

    setText: (tabId, text) => {
      update(tabId, { text });
    },

    insertText: (tabId, text) => {
      update(tabId, (tab) => ({
        text: tab.text.trim() === "" ? text : `${tab.text.trimEnd()}\n\n${text}`,
        revision: tab.revision + 1,
      }));
    },

    rename: (tabId, title) => {
      const trimmed = title.trim();
      if (trimmed) update(tabId, { title: trimmed });
    },

    setRowLimit: (tabId, rowLimit) => {
      if (Number.isInteger(rowLimit) && rowLimit >= 1 && rowLimit <= MAX_ROW_LIMIT)
        update(tabId, { rowLimit });
    },

    show: (tabId, shown) => {
      update(tabId, { shown });
    },

    updateSplit: async (tabId) => {
      const tab = get().tabs[tabId];
      if (!tab) return [];
      if (tab.split?.text === tab.text) return tab.split.ranges;
      const ranges = await sqlSplit(tab.sessionId, tab.text);
      // Keep it only if the text did not change meanwhile.
      update(tabId, (current) =>
        current.text === tab.text ? { split: { text: tab.text, ranges } } : {},
      );
      return ranges;
    },

    run: async (tabId, selection, mode) => {
      const tab = get().tabs[tabId];
      if (!tab || tab.running) return;
      let statements: StatementRun[];
      try {
        const selected = selection.from !== selection.to;
        const ranges = selected ? [] : await get().updateSplit(tabId);
        const selectionRanges = selected
          ? await sqlSplit(tab.sessionId, tab.text.slice(selection.from, selection.to))
          : [];
        statements = plan(tab.text, selection, mode, ranges, selectionRanges);
      } catch (caught) {
        update(tabId, { notice: toAppError(caught) });
        return;
      }
      if (statements.length === 0) {
        update(tabId, { notice: noStatement() });
        return;
      }
      await execute(tabId, statements, false);
    },

    confirm: async (tabId) => {
      const pending = get().tabs[tabId]?.confirmation;
      if (!pending) return;
      await execute(tabId, pending.statements, true);
    },

    dismissConfirmation: (tabId) => {
      update(tabId, { confirmation: null });
    },

    cancel: async (tabId) => {
      const tab = get().tabs[tabId];
      if (!tab?.running) return;
      updateRun(tabId, (run) => ({ ...run, cancelRequested: true }));
      try {
        await queryCancel(tab.sessionId, tabId);
      } catch (caught) {
        update(tabId, { notice: toAppError(caught) });
      }
    },

    closeTab: async (tabId) => {
      const tab = get().tabs[tabId];
      if (!tab) return;
      set({ tabs: Object.fromEntries(Object.entries(get().tabs).filter(([k]) => k !== tabId)) });
      useWorkspace.getState().close(tab.sessionId, tabId);
      try {
        await tabClose(tab.sessionId, tabId);
      } catch (caught) {
        // The session may already be gone, which closed the connection too.
        const error = toAppError(caught);
        if (error.code !== "not_found") {
          useSessions.setState({
            notice: `The tab's connection did not close cleanly: ${error.message}`,
          });
        }
      }
    },

    dropSession: dropSessionTabs,
  };
});
