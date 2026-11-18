import { Button } from "@roost/ui";
import type { useTableEditing } from "./useTableEditing";

type Editing = ReturnType<typeof useTableEditing>;

const MOD = typeof navigator !== "undefined" && /Mac/.test(navigator.platform) ? "⌘" : "Ctrl+";

/** Row actions on the selection (task 7.9). */
export function EditToolbar({ editing }: { editing: Editing }) {
  const nothing = editing.selection === null;
  return (
    <div
      role="toolbar"
      aria-label="Edit rows"
      className="flex items-center gap-1 border-b border-zinc-800 px-3 py-1 text-xs"
    >
      <Button variant="ghost" className="py-0.5 text-xs" onClick={editing.addRow}>
        Add row
      </Button>
      <Button
        variant="ghost"
        className="py-0.5 text-xs"
        disabled={nothing}
        onClick={editing.duplicate}
      >
        Duplicate
      </Button>
      <Button
        variant="ghost"
        className="py-0.5 text-xs"
        disabled={nothing}
        title={`Delete the selected rows (${MOD}⌫)`}
        onClick={editing.deleteSelected}
      >
        Delete rows
      </Button>
      <Button
        variant="ghost"
        className="py-0.5 text-xs"
        disabled={nothing}
        title={`Set the selected cells to NULL (${MOD}⇧⌫)`}
        onClick={editing.setNullSelected}
      >
        Set NULL
      </Button>
      <Button
        variant="ghost"
        className="py-0.5 text-xs"
        disabled={nothing}
        title="Undo the pending change of the selected cell, or of the selected rows"
        onClick={editing.revert}
      >
        Revert
      </Button>
      <span className="ml-2 text-zinc-500">
        Enter, F2 or typing edits a cell; edits stay pending until Apply.
      </span>
    </div>
  );
}

/** Pending counts with Preview/Apply and Discard, and the outcome of the last apply (7.10, 7.12). */
export function ChangesBar({ editing }: { editing: Editing }) {
  const { counts, error, notice } = editing;
  if (counts.total === 0 && !error && !notice) return null;
  const parts = [
    counts.updates > 0 && `${counts.updates} edited`,
    counts.inserts > 0 && `${counts.inserts} new`,
    counts.deletes > 0 && `${counts.deletes} deleted`,
  ].filter(Boolean);
  return (
    <div className="border-b border-amber-900 bg-amber-950/40 px-3 py-1 text-xs">
      <div className="flex items-center gap-2">
        {counts.total > 0 && (
          <>
            <span role="status" aria-label="Pending changes" className="text-amber-200">
              {parts.join(" · ")} — not applied yet
            </span>
            <Button
              variant="primary"
              className="ml-auto py-0.5 text-xs"
              disabled={editing.applying}
              onClick={() => void editing.requestApply()}
            >
              {editing.applying ? "Applying…" : "Apply changes"}
            </Button>
            <Button
              variant="ghost"
              className="py-0.5 text-xs"
              disabled={editing.applying}
              onClick={editing.discard}
            >
              Discard
            </Button>
          </>
        )}
        {counts.total === 0 && notice && (
          <span role="status" className="text-emerald-300">
            {notice}
          </span>
        )}
      </div>
      {error && (
        <p role="alert" className="mt-1 font-mono text-rose-300">
          Nothing was applied: {error.message}
          {typeof (error.detail as { operation?: unknown } | undefined)?.operation === "number" &&
            " The row that failed is marked."}
        </p>
      )}
    </div>
  );
}
