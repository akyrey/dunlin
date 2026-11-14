/** Export and copy formats as the backend takes them (`ExportFormat`). */
import type { ExportFormat, TableRef } from "../../ipc";
import type { ExportKind } from "../../ipc";

export type ExportOptions = {
  kind: ExportKind;
  delimiter: string;
  header: boolean;
  nullText: string;
  batch: number;
};

export const DEFAULT_EXPORT: ExportOptions = {
  kind: "csv",
  delimiter: ",",
  header: true,
  nullText: "",
  batch: 100,
};

export const DELIMITERS: { value: string; label: string }[] = [
  { value: ",", label: "Comma" },
  { value: ";", label: "Semicolon" },
  { value: "\t", label: "Tab" },
  { value: "|", label: "Pipe" },
];

/** `table` names the `INSERT` target; required for that kind. */
export function toFormat(options: ExportOptions, table: TableRef | null): ExportFormat | null {
  switch (options.kind) {
    case "csv":
      return {
        kind: "csv",
        delimiter: options.delimiter,
        header: options.header,
        null: options.nullText,
      };
    case "json":
      return { kind: "json" };
    case "insert":
      return table
        ? { kind: "insert", table, batch: Math.max(1, Math.floor(options.batch)) }
        : null;
  }
}
