import { useId, useState } from "react";

import { type ColumnMeta, type TableRef, type Value, exportText, toAppError } from "../../ipc";
import { copyText } from "../../lib/clipboard";
import { toTsv } from "../grid/copy";
import type { Range } from "../grid/selection";
import { DEFAULT_EXPORT, toFormat } from "./format";

type Choice = "tsv" | "tsv-header" | "csv" | "json" | "insert";

const CHOICES: { value: Choice; label: string }[] = [
  { value: "tsv", label: "TSV" },
  { value: "tsv-header", label: "TSV with headers" },
  { value: "csv", label: "CSV with headers" },
  { value: "json", label: "JSON" },
  { value: "insert", label: "SQL INSERT" },
];

type Props = {
  sessionId: string;
  columns: ColumnMeta[];
  rows: Value[][];
  /** The selected cells; everything when nothing is selected. */
  selection: Range | null;
  insertTable: TableRef;
};

/** Copies the selection, or all rows, in a format (task 8.5). Ctrl/⌘+C in the grid copies TSV. */
export function CopyAs({ sessionId, columns, rows, selection, insertTable }: Props) {
  const id = useId();
  const [status, setStatus] = useState<string | null>(null);

  const copy = async (choice: Choice) => {
    const range = selection ?? {
      top: 0,
      bottom: rows.length - 1,
      left: 0,
      right: columns.length - 1,
    };
    const picked = rows
      .slice(range.top, range.bottom + 1)
      .map((row) => row.slice(range.left, range.right + 1));
    const pickedColumns = columns.slice(range.left, range.right + 1);
    try {
      if (choice === "tsv" || choice === "tsv-header") {
        const header = choice === "tsv-header" ? columns.map((c) => c.name) : undefined;
        await copyText(toTsv(rows, range, header));
      } else {
        const format = toFormat({ ...DEFAULT_EXPORT, kind: choice }, insertTable);
        if (!format) return;
        await copyText(
          await exportText({ sessionId, columns: pickedColumns, rows: picked, format }),
        );
      }
      setStatus(`Copied ${picked.length} ${picked.length === 1 ? "row" : "rows"}`);
    } catch (caught) {
      setStatus(toAppError(caught).message);
    }
  };

  return (
    <span className="flex items-center gap-1">
      <label htmlFor={id} className="sr-only">
        Copy as
      </label>
      <select
        id={id}
        className="rounded bg-zinc-800 px-1 py-0.5 text-zinc-200"
        value=""
        onChange={(e) => {
          if (e.target.value) void copy(e.target.value as Choice);
        }}
      >
        <option value="">{selection ? "Copy selection as…" : "Copy all as…"}</option>
        {CHOICES.map((c) => (
          <option key={c.value} value={c.value}>
            {c.label}
          </option>
        ))}
      </select>
      {status && (
        <span role="status" className="text-zinc-500">
          {status}
        </span>
      )}
    </span>
  );
}
