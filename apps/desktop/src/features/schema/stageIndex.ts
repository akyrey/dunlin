/** "Create index…" from a plan hint (task 10.9, D-043): opens the table's detail tab with the
 * suggested index staged as a pending schema change. Nothing runs until it is applied there. */
import { type SuggestedIndex, schemaTable } from "../../ipc";
import { tableKey, useWorkspace } from "../workspace/workspaceStore";
import { useSchemaEdits } from "./schemaEditStore";

export async function stageSuggestedIndex(
  sessionId: string,
  suggested: SuggestedIndex,
): Promise<void> {
  const info = await schemaTable(sessionId, suggested.table);
  useWorkspace.getState().openTable(sessionId, suggested.table);
  useSchemaEdits.getState().dispatch(tableKey(suggested.table), sessionId, info, {
    type: "addObject",
    object: { kind: "index", value: suggested.index },
  });
}
