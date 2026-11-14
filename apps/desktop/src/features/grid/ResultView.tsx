import { type ReactNode, useMemo, useState } from "react";

import type { SortKey, Value } from "../../ipc";
import { rowsToJson } from "../../lib/json";
import { ReadOnlyCode } from "../editor/ReadOnlyCode";
import { CellViewer } from "./CellViewer";
import { DataGrid, type GridColumn } from "./DataGrid";
import type { CellPos } from "./selection";

export type ViewMode = "grid" | "json";

type Props = {
  columns: GridColumn[];
  rows: Value[][];
  rowOffset?: number;
  sort?: SortKey[];
  onSort?: (column: string, additive: boolean) => void;
  onFollow?: (cell: CellPos) => void;
  /** Loads a truncated cell in full; `null` when rows cannot be addressed. */
  loadFull: ((cell: CellPos) => (() => Promise<Value>) | null) | null;
  noFullReason: string;
  /** Extra controls for the toolbar (page controls, counts). */
  toolbar?: ReactNode;
};

/** A result as a grid or as JSON, with the cell viewer for the selected cell (6.9, 6.10, 6.13). */
export function ResultView({
  columns,
  rows,
  rowOffset,
  sort,
  onSort,
  onFollow,
  loadFull,
  noFullReason,
  toolbar,
}: Props) {
  const [mode, setMode] = useState<ViewMode>("grid");
  const [viewer, setViewer] = useState(false);
  const [focus, setFocus] = useState<CellPos | null>(null);
  const json = useMemo(
    () =>
      mode === "json"
        ? rowsToJson(
            columns.map((c) => c.name),
            rows,
          )
        : "",
    [mode, columns, rows],
  );
  const focusedValue = focus ? rows[focus.row]?.[focus.col] : undefined;
  const focusedColumn = focus ? columns[focus.col] : undefined;

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-zinc-800 px-2 py-1 text-xs">
        <div
          role="radiogroup"
          aria-label="Show as"
          className="flex overflow-hidden rounded border border-zinc-700"
        >
          {(["grid", "json"] as const).map((option) => (
            <button
              key={option}
              type="button"
              role="radio"
              aria-checked={mode === option}
              className={`px-2 py-0.5 ${mode === option ? "bg-zinc-700 text-zinc-100" : "text-zinc-400 hover:text-zinc-200"}`}
              onClick={() => {
                setMode(option);
              }}
            >
              {option === "grid" ? "Grid" : "JSON"}
            </button>
          ))}
        </div>
        <button
          type="button"
          aria-pressed={viewer}
          className={`rounded px-2 py-0.5 ${viewer ? "bg-zinc-700 text-zinc-100" : "text-zinc-400 hover:text-zinc-200"}`}
          onClick={() => {
            setViewer((open) => !open);
          }}
        >
          Cell viewer
        </button>
        {toolbar}
      </div>
      <div className="flex min-h-0 flex-1">
        <div className="min-w-0 flex-1">
          {mode === "grid" ? (
            <DataGrid
              columns={columns}
              rows={rows}
              rowOffset={rowOffset}
              sort={sort}
              onSort={onSort}
              onFollow={onFollow}
              onFocusCell={setFocus}
              onActivate={(cell) => {
                setFocus(cell);
                setViewer(true);
              }}
            />
          ) : (
            <div className="flex h-full flex-col">
              <p className="px-2 py-1 text-[11px] text-zinc-500">
                Integers beyond ±2^53 and decimals a JavaScript number cannot hold exactly are
                strings; binary values are base64; values cut short in the grid are cut short here
                too.
              </p>
              <div className="min-h-0 flex-1">
                <ReadOnlyCode text={json} label="Rows as JSON" language="json" />
              </div>
            </div>
          )}
        </div>
        {viewer &&
          (focus && focusedValue && focusedColumn ? (
            <CellViewer
              key={`${focus.row}:${focus.col}`}
              column={focusedColumn}
              value={focusedValue}
              loadFull={loadFull?.(focus) ?? null}
              noFullReason={noFullReason}
              onClose={() => {
                setViewer(false);
              }}
            />
          ) : (
            <aside
              aria-label="Cell viewer"
              className="w-96 shrink-0 border-l border-zinc-800 p-3 text-xs text-zinc-500"
            >
              Select a cell to see its whole value.
            </aside>
          ))}
      </div>
    </div>
  );
}
