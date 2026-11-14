/** Editor tabs survive a restart (task 5.8): each connection's tabs (title, text, row limit, in
 * tab-strip order) are kept in localStorage. This is editor content the user typed, never
 * results or secrets. */
import { useWorkspace, workspaceOf } from "../workspace/workspaceStore";
import { type EditorTab, useEditor } from "./editorStore";

const PREFIX = "dunlin.tabs.";
const SAVE_DELAY_MS = 300;

export type SavedTab = {
  title: string;
  text: string;
  rowLimit: number;
  savedQueryId?: string | null;
};

function isSavedTab(value: unknown): value is SavedTab {
  if (typeof value !== "object" || value === null) return false;
  const tab = value as Record<string, unknown>;
  return (
    typeof tab.title === "string" &&
    typeof tab.text === "string" &&
    typeof tab.rowLimit === "number"
  );
}

export function loadTabs(connectionId: string, storage: Storage = localStorage): SavedTab[] {
  try {
    const parsed: unknown = JSON.parse(storage.getItem(PREFIX + connectionId) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter(isSavedTab) : [];
  } catch {
    // Unreadable saved tabs are not worth failing over: start with an empty editor.
    return [];
  }
}

export function saveTabs(
  connectionId: string,
  tabs: SavedTab[],
  storage: Storage = localStorage,
): void {
  storage.setItem(PREFIX + connectionId, JSON.stringify(tabs));
}

export type OpenSession = { sessionId: string; connectionId: string };

/** The tabs of every open session, by connection id, in tab-strip order. A session whose last
 * tab was closed saves an empty list, so the closed tabs do not come back. */
export function snapshot(
  tabs: Record<string, EditorTab>,
  sessions: OpenSession[],
): Map<string, SavedTab[]> {
  const byConnection = new Map<string, SavedTab[]>();
  for (const { sessionId, connectionId } of sessions) {
    const order = workspaceOf(useWorkspace.getState(), sessionId).views.map((v) => v.key);
    const saved = Object.values(tabs)
      .filter((t) => t.sessionId === sessionId)
      .sort((a, b) => order.indexOf(a.tabId) - order.indexOf(b.tabId))
      .map(({ title, text, rowLimit, savedQueryId }) => ({ title, text, rowLimit, savedQueryId }));
    byConnection.set(connectionId, saved);
  }
  return byConnection;
}

/** Saves the tabs of `sessions()` shortly after they change. Returns a function that stops. */
export function persistTabs(
  sessions: () => OpenSession[],
  storage: Storage = localStorage,
): () => void {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const save = () => {
    timer = null;
    for (const [connectionId, tabs] of snapshot(useEditor.getState().tabs, sessions())) {
      try {
        saveTabs(connectionId, tabs, storage);
      } catch {
        // A full or disabled storage only loses the restore; the tabs themselves are intact.
      }
    }
  };
  const schedule = () => {
    if (timer === null) timer = setTimeout(save, SAVE_DELAY_MS);
  };
  const stopEditor = useEditor.subscribe(schedule);
  const stopWorkspace = useWorkspace.subscribe(schedule);
  return () => {
    stopEditor();
    stopWorkspace();
    if (timer !== null) clearTimeout(timer);
  };
}
