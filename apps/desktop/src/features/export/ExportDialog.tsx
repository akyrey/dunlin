import { useId, useMemo, useState } from "react";

import { Button, Modal } from "@roost/ui";
import {
  type AppError,
  type ColumnMeta,
  type ExportDone,
  type KeyArg,
  type SortKey,
  type TableRef,
  type Value,
  exportResult,
  exportTable,
  pickExportFile,
  queryCancel,
  revealFile,
  tabClose,
  toAppError,
} from "../../ipc";
import { DEFAULT_EXPORT, DELIMITERS, type ExportOptions, toFormat } from "./format";

/** A table view's whole table, as filtered and sorted. */
export type TableSource = {
  viewId: string;
  table: TableRef;
  filter?: string;
  key: KeyArg[];
  sort: SortKey[];
};

type Props = {
  sessionId: string;
  /** Suggested file name. */
  name: string;
  columns: ColumnMeta[];
  rows: Value[][];
  /** Offered when the rows come from a table view. */
  table?: TableSource;
  onClose: () => void;
};

type State =
  | { phase: "choose" }
  | { phase: "running"; rows: number; exportViewId: string | null }
  | { phase: "done"; done: ExportDone }
  | { phase: "failed"; error: AppError };

/** Export the rows shown or the whole table to a file (task 8.4). */
export function ExportDialog({ sessionId, name, columns, rows, table, onClose }: Props) {
  const ids = {
    kind: useId(),
    delimiter: useId(),
    nullText: useId(),
    batch: useId(),
    target: useId(),
  };
  const [options, setOptions] = useState<ExportOptions>(DEFAULT_EXPORT);
  const [whole, setWhole] = useState(table !== undefined);
  const [targetName, setTargetName] = useState(table?.table.name ?? name);
  const [state, setState] = useState<State>({ phase: "choose" });
  const insertTarget = useMemo<TableRef | null>(() => {
    if (table && (whole || targetName === table.table.name)) return table.table;
    const trimmed = targetName.trim();
    return trimmed ? { namespace: { catalog: null, schema: null }, name: trimmed } : null;
  }, [table, whole, targetName]);
  const format = toFormat(options, insertTarget);
  const set = (patch: Partial<ExportOptions>) => {
    setOptions((current) => ({ ...current, ...patch }));
  };

  const run = async () => {
    if (!format) return;
    const path = await pickExportFile(options.kind, table?.table.name ?? name);
    if (!path) return;
    const exportViewId = whole && table ? `${table.viewId}:export` : null;
    setState({ phase: "running", rows: 0, exportViewId });
    try {
      const done =
        exportViewId && table
          ? await exportTable(
              {
                sessionId,
                viewId: exportViewId,
                table: table.table,
                filter: table.filter,
                key: table.key,
                sort: table.sort,
                format,
                path,
              },
              (written) => {
                setState({ phase: "running", rows: written, exportViewId });
              },
            )
          : await exportResult({ sessionId, columns, rows, format, path });
      setState({ phase: "done", done });
    } catch (caught) {
      setState({ phase: "failed", error: toAppError(caught) });
    } finally {
      if (exportViewId) {
        tabClose(sessionId, exportViewId).catch(() => {
          // The session may be gone, which closed the export's connection too.
        });
      }
    }
  };

  const footer =
    state.phase === "running" ? (
      state.exportViewId ? (
        <Button
          variant="danger"
          onClick={() => {
            if (state.exportViewId) void queryCancel(sessionId, state.exportViewId);
          }}
        >
          Cancel export
        </Button>
      ) : null
    ) : state.phase === "done" ? (
      <>
        <Button onClick={() => void revealFile(state.done.path)}>Show in Finder</Button>
        <Button variant="primary" onClick={onClose}>
          Done
        </Button>
      </>
    ) : (
      <>
        <Button onClick={onClose}>Cancel</Button>
        <Button variant="primary" disabled={!format} onClick={() => void run()}>
          Export…
        </Button>
      </>
    );

  return (
    <Modal
      title="Export"
      onClose={state.phase === "running" ? () => undefined : onClose}
      footer={footer}
    >
      {state.phase === "running" && (
        <p role="status" className="text-sm text-zinc-200">
          Exporting… {state.rows.toLocaleString("en-US")} rows written
        </p>
      )}
      {state.phase === "done" && (
        <p role="status" className="break-all text-sm text-emerald-300">
          Wrote {Number(state.done.rowsWritten).toLocaleString("en-US")} rows to {state.done.path}
        </p>
      )}
      {(state.phase === "choose" || state.phase === "failed") && (
        <div className="flex flex-col gap-3 text-sm">
          {state.phase === "failed" && (
            <p role="alert" className="text-rose-300">
              {state.error.message}
            </p>
          )}
          {table && (
            <fieldset className="flex flex-col gap-1">
              <legend className="mb-1 text-xs text-zinc-400">Rows</legend>
              <label className="flex items-center gap-2">
                <input
                  type="radio"
                  checked={whole}
                  onChange={() => {
                    setWhole(true);
                  }}
                />
                The whole table{table.filter ? ", filtered" : ""}, streamed from the database
              </label>
              <label className="flex items-center gap-2">
                <input
                  type="radio"
                  checked={!whole}
                  onChange={() => {
                    setWhole(false);
                  }}
                />
                The {rows.length.toLocaleString("en-US")} rows shown
              </label>
            </fieldset>
          )}
          <label className="flex items-center gap-2" htmlFor={ids.kind}>
            Format
            <select
              id={ids.kind}
              className="input"
              value={options.kind}
              onChange={(e) => {
                set({ kind: e.target.value as ExportOptions["kind"] });
              }}
            >
              <option value="csv">CSV</option>
              <option value="json">JSON</option>
              <option value="insert">SQL INSERT statements</option>
            </select>
          </label>
          {options.kind === "csv" && (
            <div className="flex flex-wrap items-center gap-3">
              <label className="flex items-center gap-2" htmlFor={ids.delimiter}>
                Delimiter
                <select
                  id={ids.delimiter}
                  className="input"
                  value={options.delimiter}
                  onChange={(e) => {
                    set({ delimiter: e.target.value });
                  }}
                >
                  {DELIMITERS.map((d) => (
                    <option key={d.value} value={d.value}>
                      {d.label}
                    </option>
                  ))}
                </select>
              </label>
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={options.header}
                  onChange={(e) => {
                    set({ header: e.target.checked });
                  }}
                />
                Header row
              </label>
              <label className="flex items-center gap-2" htmlFor={ids.nullText}>
                NULL as
                <input
                  id={ids.nullText}
                  className="input w-20"
                  placeholder="(empty)"
                  value={options.nullText}
                  onChange={(e) => {
                    set({ nullText: e.target.value });
                  }}
                />
              </label>
            </div>
          )}
          {options.kind === "insert" && (
            <div className="flex flex-wrap items-center gap-3">
              {!(table && whole) && (
                <label className="flex items-center gap-2" htmlFor={ids.target}>
                  Table
                  <input
                    id={ids.target}
                    className="input"
                    value={targetName}
                    onChange={(e) => {
                      setTargetName(e.target.value);
                    }}
                  />
                </label>
              )}
              <label className="flex items-center gap-2" htmlFor={ids.batch}>
                Rows per INSERT
                <input
                  id={ids.batch}
                  type="number"
                  min={1}
                  className="input w-24"
                  value={options.batch}
                  onChange={(e) => {
                    set({ batch: Number(e.target.value) });
                  }}
                />
              </label>
            </div>
          )}
          {options.kind === "json" && (
            <p className="text-xs text-zinc-500">
              Numbers a JavaScript number cannot hold exactly are written as strings; binary values
              as base64.
            </p>
          )}
          {!whole &&
            rows.some((row) => row.some((v) => "truncated" in v && v.truncated === true)) && (
              <p className="text-xs text-amber-300">
                Some values were cut short for display; export the whole table to write them in
                full.
              </p>
            )}
        </div>
      )}
    </Modal>
  );
}
