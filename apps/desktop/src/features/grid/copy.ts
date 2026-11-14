/** Copying a selected range as tab-separated values (task 6.5). */
import type { Value } from "../../ipc";
import { rawText } from "../../lib/value";
import type { Range } from "./selection";

/** A field needs quoting when it holds a tab, a line break or a quote (spreadsheet rules). */
function field(value: Value | undefined): string {
  if (!value || value.t === "null") return "";
  const text = rawText(value);
  return /[\t\n\r"]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

export function toTsv(rows: Value[][], range: Range, header?: string[]): string {
  const lines: string[] = [];
  if (header) lines.push(header.slice(range.left, range.right + 1).join("\t"));
  for (let row = range.top; row <= range.bottom; row++) {
    const cells = rows[row] ?? [];
    const line: string[] = [];
    for (let col = range.left; col <= range.right; col++) line.push(field(cells[col]));
    lines.push(line.join("\t"));
  }
  return lines.join("\n");
}
