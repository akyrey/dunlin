/** What the tree's context menu does with a table or view (task 4.10). */
import { type TableRef, schemaSnippets } from "../../ipc";
import { copyText } from "../../lib/clipboard";
import { useEditor } from "../editor/editorStore";
import { useSessions } from "../sessions/sessionsStore";
import { EDITOR_VIEW, useWorkspace } from "../workspace/workspaceStore";

export type ObjectAction = "details" | "copy-name" | "copy-qualified" | "select";

export const OBJECT_ACTIONS: { action: ObjectAction; label: string }[] = [
  { action: "details", label: "Open details" },
  { action: "copy-name", label: "Copy name" },
  { action: "copy-qualified", label: "Copy qualified name" },
  { action: "select", label: "Generate SELECT" },
];

export type ActionTarget = { connectionId: string; sessionId: string; table: TableRef };

/** Shows the target's session in the main area. */
function focus(target: ActionTarget): void {
  useSessions.getState().activate(target.connectionId);
}

export async function runObjectAction(action: ObjectAction, target: ActionTarget): Promise<void> {
  const { sessionId, table } = target;
  switch (action) {
    case "details":
      focus(target);
      useWorkspace.getState().openTable(sessionId, table);
      return;
    case "copy-name":
      await copyText(table.name);
      return;
    case "copy-qualified":
      await copyText((await schemaSnippets(sessionId, table)).qualifiedName);
      return;
    case "select": {
      const { select } = await schemaSnippets(sessionId, table);
      useEditor.getState().insertText(sessionId, select);
      focus(target);
      useWorkspace.getState().activate(sessionId, EDITOR_VIEW);
      return;
    }
  }
}
