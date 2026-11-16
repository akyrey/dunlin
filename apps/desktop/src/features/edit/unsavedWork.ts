import { useEditor } from "../editor/editorStore";
import { useEdits } from "./editStore";

/** Work that is lost when the app quits or restarts. */
export type UnsavedWork = { changes: number; transactions: number };

export function unsavedWork(): UnsavedWork {
  const changes = Object.values(useEdits.getState().pending).reduce((sum, v) => sum + v.count, 0);
  const transactions = Object.values(useEditor.getState().tabs).filter(
    (t) => t.transaction.open,
  ).length;
  return { changes, transactions };
}

export const hasUnsavedWork = (work: UnsavedWork): boolean =>
  work.changes > 0 || work.transactions > 0;

/** What will be lost, in a sentence or two; empty when nothing is. */
export function describeUnsavedWork({ changes, transactions }: UnsavedWork): string {
  const parts = [
    changes > 0 &&
      `${changes} ${changes === 1 ? "change has" : "changes have"} not been applied and will be lost.`,
    transactions > 0 &&
      `${transactions} ${transactions === 1 ? "tab has an open transaction" : "tabs have open transactions"}, which will be rolled back.`,
  ].filter(Boolean);
  return parts.join(" ");
}
