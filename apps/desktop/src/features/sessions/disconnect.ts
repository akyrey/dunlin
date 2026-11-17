import { toAppError } from "../../ipc";
import { pendingViews, useEdits } from "../edit/editStore";
import { useSessions } from "./sessionsStore";

const UNAPPLIED = "Apply or discard the unapplied table changes before disconnecting.";

/** Closes a connection's session unless it has unapplied edits; returns why it did not. */
export async function disconnect(connectionId: string): Promise<string | null> {
  const sessionId = useSessions.getState().sessions[connectionId]?.sessionId;
  if (sessionId && pendingViews(useEdits.getState(), sessionId).length > 0) return UNAPPLIED;
  try {
    await useSessions.getState().close(connectionId);
    return null;
  } catch (caught) {
    return toAppError(caught).message;
  }
}
