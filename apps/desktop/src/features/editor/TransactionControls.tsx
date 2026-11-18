import { Button, toast } from "@roost/ui";
import { toAppError } from "../../ipc";
import { type EditorTab, useEditor } from "./editorStore";

/** Auto-commit or manual transactions for one tab, with Commit and Rollback while a
 * transaction is open (deferred feature 2). */
export function TransactionControls({ tab }: { tab: EditorTab }) {
  const { manual, open } = tab.transaction;
  const actions = useEditor.getState();
  return (
    <span className="flex items-center gap-2">
      <label
        className="flex items-center gap-1 text-xs text-zinc-400"
        title={
          open
            ? "Commit or roll back before going back to auto-commit"
            : "In manual mode, statements run in a transaction you commit or roll back"
        }
      >
        <input
          type="checkbox"
          checked={manual}
          disabled={open || tab.running}
          onChange={(e) => {
            actions.setManual(tab.tabId, e.target.checked).catch((caught: unknown) => {
              toast.error(toAppError(caught).message);
            });
          }}
        />
        Manual commit
      </label>
      {open && (
        <>
          <span
            role="status"
            aria-label="Transaction"
            className="rounded bg-amber-900/60 px-1.5 py-0.5 text-[11px] font-semibold uppercase text-amber-300"
          >
            Transaction open
          </span>
          <Button
            className="py-0.5 text-xs"
            disabled={tab.running}
            onClick={() => void actions.finishTransaction(tab.tabId, true)}
          >
            Commit
          </Button>
          <Button
            variant="danger"
            className="py-0.5 text-xs"
            disabled={tab.running}
            onClick={() => void actions.finishTransaction(tab.tabId, false)}
          >
            Rollback
          </Button>
        </>
      )}
    </span>
  );
}
