import { type SavedQuery, toAppError } from "../../ipc";
import { useEditor } from "../editor/editorStore";
import { useSessions } from "../sessions/sessionsStore";
import { useWorkspace } from "../workspace/workspaceStore";

/** Opens a saved query in a tab of its connection (or the active one, for a global query), or
 * shows the tab already open for it. Resolves with a message when it cannot. */
export async function openSavedQuery(
  query: SavedQuery,
  activeConnectionId: string | null,
): Promise<string | null> {
  const connectionId = query.connectionId ?? activeConnectionId;
  if (!connectionId) return "Open a connection to run this query in.";
  try {
    const session = await useSessions.getState().open(connectionId);
    const existing = Object.values(useEditor.getState().tabs).find(
      (t) => t.sessionId === session.sessionId && t.savedQueryId === query.id,
    );
    if (existing) {
      useWorkspace.getState().activate(session.sessionId, existing.tabId);
      return null;
    }
    useEditor
      .getState()
      .openTab(session.sessionId, { title: query.name, text: query.sql, savedQueryId: query.id });
    return null;
  } catch (caught) {
    return toAppError(caught).message;
  }
}
