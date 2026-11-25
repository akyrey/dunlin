/** Preview and apply of a table's staged schema changes (task 9.12). */
import { useState } from "react";

import {
  type SchemaEditArgs,
  type SchemaEditPreview,
  type TableInfo,
  schemaEditApply,
  schemaEditPreview,
  toAppError,
} from "../../ipc";
import { useCompletion } from "../editor/completionStore";
import { useWorkspace } from "../workspace/workspaceStore";
import { pendingFor, useSchemaEdits } from "./schemaEditStore";
import { changeCount, toChanges } from "./schemaPending";
import { useSchema } from "./schemaStore";

/** Why Apply failed; with `applied`, statements that ran before it stayed (MySQL). */
export type ApplyError = {
  message: string;
  failedStatement: string | null;
  applied: string[];
};

type Options = {
  sessionId: string;
  viewKey: string;
  info: TableInfo;
  /** Reloads the tab's description, after the table changed. */
  onChanged: () => void;
};

function failureOf(caught: unknown, preview: SchemaEditPreview | null): ApplyError {
  const error = toAppError(caught);
  const detail = error.detail as { statement?: number | null; applied?: number } | undefined;
  const plan = preview?.statements.slice(preview.statementOffset) ?? [];
  const index = typeof detail?.statement === "number" ? detail.statement : null;
  return {
    message: error.message,
    failedStatement: index === null ? null : (plan[index] ?? null),
    applied: plan.slice(0, detail?.applied ?? 0),
  };
}

export function useSchemaEditing({ sessionId, viewKey, info, onChanged }: Options) {
  const pending = useSchemaEdits((state) => pendingFor(state, viewKey));
  const [preview, setPreview] = useState<SchemaEditPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApplyError | null>(null);
  const count = changeCount(pending);

  const args = (confirmed: boolean): SchemaEditArgs => ({
    sessionId,
    edit: { original: info, changes: toChanges(info, pending) },
    confirmed,
  });

  const afterChange = () => {
    void useSchema.getState().reload(sessionId);
    void useCompletion.getState().reload(sessionId);
    if (pending.tableName !== undefined) {
      const renamed = { ...info.table, name: pending.tableName };
      useWorkspace.getState().openTable(sessionId, renamed);
      useWorkspace.getState().close(sessionId, viewKey);
    }
    useSchemaEdits.getState().clear(viewKey, sessionId);
    onChanged();
  };

  const openPreview = async () => {
    setBusy(true);
    setError(null);
    try {
      setPreview(await schemaEditPreview(args(false)));
    } catch (caught) {
      setError(failureOf(caught, null));
    } finally {
      setBusy(false);
    }
  };

  const apply = async () => {
    if (!preview) return;
    setBusy(true);
    try {
      await schemaEditApply(args(preview.destructive));
      setPreview(null);
      afterChange();
    } catch (caught) {
      const failure = failureOf(caught, preview);
      setPreview(null);
      setError(failure);
      // Part of the plan stayed applied: the staged changes no longer match the table.
      if (failure.applied.length > 0) afterChange();
    } finally {
      setBusy(false);
    }
  };

  return {
    pending,
    count,
    preview,
    busy,
    error,
    openPreview,
    apply,
    cancelPreview: () => {
      setPreview(null);
    },
    dismissError: () => {
      setError(null);
    },
    discard: () => {
      useSchemaEdits.getState().clear(viewKey, sessionId);
      setError(null);
    },
  };
}

export type SchemaEditing = ReturnType<typeof useSchemaEditing>;
