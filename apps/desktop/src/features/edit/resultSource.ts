import type { TableInfo } from "../../ipc";

/** Why the rows of a single-table query cannot be edited, or `null` when they can. The backend
 * found the table; the result's columns must be that table's own, once each, and include its
 * row key so every row can be addressed. */
export function resultEditReason(info: TableInfo | null, columnNames: string[]): string | null {
  if (!info) return null;
  const known = new Set(info.columns.map((c) => c.name));
  const unknown = columnNames.find((name) => !known.has(name));
  if (unknown !== undefined) return `Read-only: ${unknown} is not a column of ${info.table.name}.`;
  if (new Set(columnNames).size !== columnNames.length)
    return "Read-only: a column appears more than once.";
  const missing = info.rowKey?.find((column) => !columnNames.includes(column));
  if (missing !== undefined)
    return `Read-only: select ${missing} too, so the rows can be addressed.`;
  return null;
}
