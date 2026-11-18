import { type ReactNode, useEffect, useMemo, useReducer, useState } from "react";

import {
  type AppError,
  type TableInfo,
  type TableRef,
  type Value,
  changesApply,
  changesPreview,
  toAppError,
} from "../../ipc";
import type { GridColumn, GridEditing } from "../grid/DataGrid";
import type { CellPos, Range } from "@roost/grid";
import { useSettings } from "../settings/settingsStore";
import { useEdits } from "./editStore";
import { editText, editorKind, formatHint, parseInput, toggled } from "./editors";
import { EMPTY, type Source, counts, reduce, toOperations } from "./pending";
import { PreviewDialog } from "./PreviewDialog";
import { UnsavedDialog } from "./UnsavedDialog";
import { DEFAULT_VALUE, type DisplayRow, displayRows } from "./view";

export type EditingProfile = { name: string; color: string | null; alwaysPreview: boolean };

type Options = {
  sessionId: string;
  viewId: string;
  viewKey: string;
  table: TableRef;
  info: TableInfo | null;
  columns: GridColumn[];
  page: Value[][];
  readOnly: boolean;
  profile: EditingProfile;
  /** The page must be fetched again (after a successful apply). */
  onApplied: () => void;
  /** A reason the view is read-only besides the table's own (a query result's columns). */
  blocked?: string | null;
};

const NULL: Value = { t: "null" };

/** Why a view cannot be edited, or `null` when it can (rules of task 7.x, checked again by the
 * backend). */
export function readOnlyReason(info: TableInfo | null, readOnly: boolean): string | null {
  if (readOnly) return "Read-only connection: rows cannot be edited.";
  if (!info) return "Loading the table's description…";
  if (info.kind !== "table") return "Views cannot be edited.";
  if (!info.rowKey)
    return "Read-only: the table has no primary key or unique not-null key to address rows by.";
  return null;
}

function writable(info: TableInfo | null, column: string): boolean {
  const described = info?.columns.find((c) => c.name === column);
  return (
    described !== undefined &&
    !described.generated &&
    described.default !== "GENERATED ALWAYS AS IDENTITY"
  );
}

/** Staged editing for one table view: pending state, grid callbacks, apply with preview, and
 * guards for anything that would lose the edits (tasks 7.7–7.13). */
export function useTableEditing(options: Options) {
  const { sessionId, viewId, viewKey, table, info, columns, page, readOnly, profile, onApplied } =
    options;
  const [pending, dispatch] = useReducer(reduce, EMPTY);
  const [selection, setSelection] = useState<Range | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [preview, setPreview] = useState<string[] | null>(null);
  const [applying, setApplying] = useState(false);
  const [guard, setGuard] = useState<{ action: string; run: () => void } | null>(null);
  const reason = readOnlyReason(info, readOnly) ?? options.blocked ?? null;
  const policy = useSettings((s) => s.settings.previewPolicy);
  // The profile or the settings can insist on the preview; then it cannot be skipped.
  const alwaysPreview = profile.alwaysPreview || policy === "always";
  const editable = reason === null;
  const names = useMemo(() => columns.map((c) => c.name), [columns]);
  const rows = useMemo(
    () => displayRows(page, names, info?.rowKey ?? null, pending),
    [page, names, info, pending],
  );
  const total = counts(pending);

  useEffect(() => {
    useEdits.getState().setPending(viewKey, sessionId, total.total);
  }, [viewKey, sessionId, total.total]);
  useEffect(
    () => () => {
      useEdits.getState().setPending(viewKey, sessionId, 0);
    },
    [viewKey, sessionId],
  );

  const at = (cell: CellPos): { row: DisplayRow; column: GridColumn; value: Value } | null => {
    const row = rows[cell.row];
    const column = columns[cell.col];
    const value = row?.values[cell.col];
    return row && column && value ? { row, column, value } : null;
  };
  /** What the cell held before any pending edit (`NULL` for a defaulted insert). */
  const originalOf = (row: DisplayRow, column: string, current: Value): Value => {
    const original = row.originals[column] ?? current;
    return original === DEFAULT_VALUE ? NULL : original;
  };

  const commit = (cell: CellPos, value: Value) => {
    const found = at(cell);
    if (!found || !writable(info, found.column.name)) return;
    const { row, column, value: current } = found;
    setFailed(null);
    if (row.kind === "inserted") {
      dispatch({ type: "editInsert", tempId: row.id, column: column.name, value });
    } else if (row.key) {
      dispatch({
        type: "edit",
        row: row.id,
        key: row.key,
        column: column.name,
        value,
        original: originalOf(row, column.name, current),
      });
    }
  };

  const deleteRows = (indexes: number[]) => {
    const chosen = indexes.map((i) => rows[i]).filter((r): r is DisplayRow => r !== undefined);
    const existing = chosen.filter((r) => r.kind === "existing" && r.key !== null);
    dispatch({ type: "delete", rows: existing.map((r) => ({ row: r.id, key: r.key ?? [] })) });
    dispatch({
      type: "deleteInsert",
      tempIds: chosen.filter((r) => r.kind === "inserted").map((r) => r.id),
    });
  };

  const edit: GridEditing | undefined = editable
    ? {
        mark: (index) => {
          const row = rows[index];
          return {
            inserted: row?.kind === "inserted",
            deleted: row?.deleted ?? false,
            edited: row?.edited ?? [],
            originals: row?.originals ?? {},
            failed: row !== undefined && row.id === failed,
          };
        },
        kind: (cell) => {
          const found = at(cell);
          if (!found || !writable(info, found.column.name)) return "none";
          return editorKind(found.column, originalOf(found.row, found.column.name, found.value));
        },
        initialText: (cell) => {
          const found = at(cell);
          return found && found.value !== DEFAULT_VALUE ? editText(found.value) : "";
        },
        parse: (cell, text) => {
          const found = at(cell);
          if (!found) return { error: "No such cell" };
          return parseInput(
            text,
            found.column,
            originalOf(found.row, found.column.name, found.value),
          );
        },
        hint: (cell) => {
          const found = at(cell);
          if (!found) return undefined;
          const original = originalOf(found.row, found.column.name, found.value);
          return editorKind(found.column, original) === "temporal"
            ? formatHint(found.column, original)
            : undefined;
        },
        onCommit: commit,
        onToggle: (cell) => {
          const found = at(cell);
          if (found) commit(cell, toggled(found.value));
        },
        onEditLong: () => undefined,
        onSetNull: (cells) => {
          for (const cell of cells) commit(cell, NULL);
        },
        onDeleteRows: deleteRows,
      }
    : undefined;

  const selectedRows = (): number[] =>
    selection
      ? Array.from({ length: selection.bottom - selection.top + 1 }, (_, i) => selection.top + i)
      : [];

  const duplicate = () => {
    for (const index of selectedRows()) {
      const row = rows[index];
      if (!row) continue;
      const values: Record<string, Value> = {};
      for (const [i, name] of names.entries()) {
        const value = row.values[i];
        // Key columns are left to the database (a new key), generated ones cannot be written.
        if (
          !value ||
          value === DEFAULT_VALUE ||
          info?.rowKey?.includes(name) ||
          !writable(info, name)
        )
          continue;
        values[name] = value;
      }
      dispatch({ type: "insert", tempId: crypto.randomUUID(), values });
    }
  };

  const revert = () => {
    for (const index of selectedRows()) {
      const row = rows[index];
      if (!row) continue;
      if (row.kind === "inserted") dispatch({ type: "deleteInsert", tempIds: [row.id] });
      else {
        // One cell of a row that is not deleted: revert that cell; otherwise the whole row.
        const single =
          selection !== null &&
          selection.left === selection.right &&
          selection.top === selection.bottom;
        const column = names[selection?.left ?? -1];
        if (single && column && !row.deleted) dispatch({ type: "revertCell", row: row.id, column });
        else dispatch({ type: "revertRow", row: row.id });
      }
    }
  };

  const sourceRow = (source: Source | undefined): string | null =>
    !source ? null : source.kind === "insert" ? source.tempId : source.row;

  const apply = async (): Promise<boolean> => {
    const { operations, sources } = toOperations(pending);
    setApplying(true);
    setError(null);
    try {
      const result = await changesApply({ sessionId, viewId, table, operations });
      const inserted = pending.inserts.length;
      dispatch({ type: "discard" });
      setFailed(null);
      setPreview(null);
      setNotice(
        `Applied ${result.statements} ${result.statements === 1 ? "statement" : "statements"}.` +
          (inserted > 0
            ? ` ${inserted} new ${inserted === 1 ? "row" : "rows"} may be on another page.`
            : ""),
      );
      onApplied();
      return true;
    } catch (caught) {
      const failure = toAppError(caught);
      const detail = failure.detail as { operation?: number | null } | undefined;
      const index = typeof detail?.operation === "number" ? detail.operation : null;
      setError(failure);
      setFailed(index === null ? null : sourceRow(sources[index]));
      setPreview(null);
      return false;
    } finally {
      setApplying(false);
    }
  };

  const requestApply = async () => {
    setNotice(null);
    const mustPreview = alwaysPreview || !useEdits.getState().previewSkipped[sessionId];
    if (!mustPreview) {
      await apply();
      return;
    }
    try {
      setPreview(
        await changesPreview({
          sessionId,
          viewId,
          table,
          operations: toOperations(pending).operations,
        }),
      );
    } catch (caught) {
      setError(toAppError(caught));
    }
  };

  /** Runs `run` now, or after the user applies or discards the pending edits. */
  const guarded = (action: string, run: () => void) => {
    if (total.total === 0) run();
    else setGuard({ action, run });
  };

  const dialogs: ReactNode = (
    <>
      {preview && (
        <PreviewDialog
          statements={preview}
          connectionName={profile.name}
          connectionColor={profile.color}
          alwaysPreview={alwaysPreview}
          applying={applying}
          onApply={(skip) => {
            if (skip) useEdits.getState().skipPreview(sessionId);
            void apply();
          }}
          onCancel={() => {
            setPreview(null);
          }}
        />
      )}
      {guard && (
        <UnsavedDialog
          count={total.total}
          action={guard.action}
          onApply={() => {
            const { run } = guard;
            setGuard(null);
            void apply().then((ok) => {
              if (ok) run();
            });
          }}
          onDiscard={() => {
            const { run } = guard;
            setGuard(null);
            dispatch({ type: "discard" });
            run();
          }}
          onCancel={() => {
            setGuard(null);
          }}
        />
      )}
    </>
  );

  return {
    editable,
    reason,
    rows,
    edit,
    counts: total,
    error,
    notice,
    applying,
    selection,
    setSelection,
    dialogs,
    guarded,
    addRow: () => {
      dispatch({ type: "insert", tempId: crypto.randomUUID() });
    },
    duplicate,
    deleteSelected: () => {
      deleteRows(selectedRows());
    },
    setNullSelected: () => {
      if (!selection || !edit) return;
      const cells: CellPos[] = [];
      for (let row = selection.top; row <= selection.bottom; row++) {
        for (let col = selection.left; col <= selection.right; col++) cells.push({ row, col });
      }
      edit.onSetNull(cells);
    },
    revert,
    discard: () => {
      dispatch({ type: "discard" });
      setFailed(null);
      setError(null);
    },
    requestApply,
  };
}
