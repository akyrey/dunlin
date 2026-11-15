import { useEffect, useMemo, useState } from "react";

import {
  type ColumnMeta,
  type TableInfo,
  type TableRef,
  type Value,
  schemaTable,
  toAppError,
} from "../../ipc";
import { gridColumns } from "../data/model";
import { ResultView } from "../grid/ResultView";
import { ChangesBar, EditToolbar } from "./EditBars";
import { resultEditReason } from "./resultSource";
import { type EditingProfile, useTableEditing } from "./useTableEditing";

type Props = {
  sessionId: string;
  /** The editor tab: its connection ran the query, and the edits apply there too. */
  tabId: string;
  source: TableRef;
  columns: ColumnMeta[];
  rows: Value[][];
  readOnly: boolean;
  profile: EditingProfile;
  /** Runs the statement again once the edits are applied. */
  onApplied: () => void;
};

/** The rows of a plain single-table query, editable like the table's own (deferred feature 1):
 * staged, previewed as SQL and applied in one transaction. */
export function EditableResult(props: Props) {
  const { sessionId, tabId, source, rows, readOnly, profile, onApplied } = props;
  const [info, setInfo] = useState<TableInfo | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    let stale = false;
    schemaTable(sessionId, source).then(
      (described) => {
        if (!stale) setInfo(described);
      },
      (caught: unknown) => {
        if (!stale) setLoadError(toAppError(caught).message);
      },
    );
    return () => {
      stale = true;
    };
  }, [sessionId, source]);

  const columns = useMemo(() => gridColumns(props.columns, info), [props.columns, info]);
  const blocked = loadError
    ? `Read-only: the table could not be described (${loadError}).`
    : resultEditReason(
        info,
        columns.map((c) => c.name),
      );
  const editing = useTableEditing({
    sessionId,
    viewId: tabId,
    viewKey: tabId,
    table: source,
    info,
    columns,
    page: rows,
    readOnly,
    profile,
    onApplied,
    blocked,
  });
  const shownRows = useMemo(() => editing.rows.map((r) => r.values), [editing.rows]);

  return (
    <div className="flex h-full flex-col">
      {editing.editable ? (
        <EditToolbar editing={editing} />
      ) : (
        info && (
          <p className="border-b border-zinc-800 px-3 py-1 text-xs text-zinc-500" role="note">
            {editing.reason}
          </p>
        )
      )}
      <ChangesBar editing={editing} />
      <div className="min-h-0 flex-1">
        <ResultView
          columns={columns}
          rows={shownRows}
          resetKey={rows}
          loadFull={null}
          exportSource={{ sessionId, name: source.name, columns: props.columns }}
          noFullReason="Results of a statement in the editor cannot be fetched again by row; open the table from the tree to load whole values."
          onSelectionChange={editing.setSelection}
          edit={editing.edit}
        />
      </div>
      {editing.dialogs}
    </div>
  );
}
