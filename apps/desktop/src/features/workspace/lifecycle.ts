/** Forgets the frontend state of sessions that ended (closed, deleted or lost). */
import { useCompletion } from "../editor/completionStore";
import { useEditor } from "../editor/editorStore";
import { useSchema } from "../schema/schemaStore";
import { useWorkspace } from "./workspaceStore";

export function forgetEndedSessions(open: Set<string>): void {
  const known = new Set([
    ...Object.keys(useSchema.getState().trees),
    ...Object.keys(useWorkspace.getState().workspaces),
    ...Object.keys(useCompletion.getState().defaults),
    ...Object.values(useEditor.getState().tabs).map((t) => t.sessionId),
  ]);
  for (const sessionId of known) {
    if (open.has(sessionId)) continue;
    useSchema.getState().dropSession(sessionId);
    useWorkspace.getState().dropSession(sessionId);
    useCompletion.getState().dropSession(sessionId);
    useEditor.getState().dropSession(sessionId);
  }
}
