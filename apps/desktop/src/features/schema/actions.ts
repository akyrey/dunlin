/** What the tree's context menu does with a table or view (task 4.10). */
import { type TableRef, schemaSnippets } from "../../ipc";
import { copyWithToast } from "../../lib/copy";
import { useEditor } from "../editor/editorStore";
import { useSessions } from "../sessions/sessionsStore";
import { useWorkspace } from "../workspace/workspaceStore";

export type ObjectAction = "data" | "details" | "copy-name" | "copy-qualified" | "select";

export const OBJECT_ACTIONS: { action: ObjectAction; label: string }[] = [
  { action: "data", label: "Open data" },
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
    case "data":
      focus(target);
      useWorkspace.getState().openData(sessionId, table);
      return;
    case "details":
      focus(target);
      useWorkspace.getState().openTable(sessionId, table);
      return;
    case "copy-name":
      await copyWithToast(table.name, "the name");
      return;
    case "copy-qualified":
      await copyWithToast((await schemaSnippets(sessionId, table)).qualifiedName, "the name");
      return;
    case "select": {
      const { select } = await schemaSnippets(sessionId, table);
      focus(target);
      useEditor.getState().openTab(sessionId, { title: table.name, text: select });
      return;
    }
  }
}
