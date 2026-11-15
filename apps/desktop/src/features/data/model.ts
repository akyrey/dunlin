/** The table data tab's pure parts: grid columns with key markers, sort cycling, the key of a
 * row, and where a foreign key cell leads (tasks 6.7, 6.12, 6.14). */
import type {
  ColumnMeta,
  ForeignKey,
  KeyArg,
  SortKey,
  TableInfo,
  TableRef,
  Value,
} from "../../ipc";
import { rawText } from "../../lib/value";
import type { GridColumn } from "../grid/DataGrid";

const PAGE_SIZES = [100, 200, 500, 1000];

/** The page sizes offered: the presets plus the current one (from Settings, any of 10..=10000). */
export function pageSizes(current: number): number[] {
  return [...new Set([...PAGE_SIZES, current])].sort((a, b) => a - b);
}

const qualified = (table: TableRef) =>
  [table.namespace.schema, table.name].filter(Boolean).join(".");

function foreignKeyOf(info: TableInfo | null, column: string): ForeignKey | undefined {
  return info?.foreignKeys.find((fk) => fk.columns.includes(column));
}

/** Page columns, with the table's own type names and key markers. */
export function gridColumns(page: ColumnMeta[], info: TableInfo | null): GridColumn[] {
  return page.map((column) => {
    const described = info?.columns.find((c) => c.name === column.name);
    const fk = foreignKeyOf(info, column.name);
    return {
      name: column.name,
      typeName: described?.typeName ?? column.typeName,
      category: described?.category ?? column.category,
      // The implicit row id (SQLite's rowid) is shown as the key it is.
      primaryKey:
        (info?.primaryKey?.columns.includes(column.name) ?? false) ||
        (info?.rowId !== null && info?.rowId === column.name),
      references: fk ? qualified(fk.referencedTable) : undefined,
    };
  });
}

/** Click: ascending, then descending, then off. Shift-click adds or cycles a secondary key. */
export function nextSort(current: SortKey[], column: string, additive: boolean): SortKey[] {
  const existing = current.find((s) => s.column === column);
  const cycled: SortKey | null = !existing
    ? { column, descending: false }
    : existing.descending
      ? null
      : { column, descending: true };
  if (!additive) return cycled ? [cycled] : [];
  if (!existing) return [...current, { column, descending: false }];
  return current.flatMap((s) => (s.column === column ? (cycled ? [cycled] : []) : [s]));
}

/** `column = value` for every column of `columns` in the row, or `null` if one is missing. */
function keyFrom(
  columns: string[],
  names: string[],
  row: Value[],
  as: string[] = columns,
): KeyArg[] | null {
  const key: KeyArg[] = [];
  for (const [i, column] of columns.entries()) {
    const value = row[names.indexOf(column)];
    const target = as[i];
    if (!value || target === undefined) return null;
    key.push({ column: target, value });
  }
  return key;
}

/** The row's key for fetching one of its cells again; `null` without a row key. */
export function rowKeyOf(info: TableInfo | null, names: string[], row: Value[]): KeyArg[] | null {
  const rowKey = info?.rowKey;
  return rowKey ? keyFrom(rowKey, names, row) : null;
}

/** Where the foreign key cell at `column` points: the referenced table and its matching key. */
export function followTarget(
  info: TableInfo | null,
  names: string[],
  row: Value[],
  column: string,
): { table: TableRef; match: KeyArg[] } | null {
  const fk = foreignKeyOf(info, column);
  if (!fk) return null;
  const match = keyFrom(fk.columns, names, row, fk.referencedColumns);
  if (!match || match.some((k) => k.value.t === "null")) return null;
  return { table: fk.referencedTable, match };
}

/** `customer_id = 1` for the chip that shows a match. */
export function describeMatch(match: KeyArg[]): string {
  return match
    .map((k) => `${k.column} = ${k.value.t === "null" ? "NULL" : rawText(k.value)}`)
    .join(", ");
}
