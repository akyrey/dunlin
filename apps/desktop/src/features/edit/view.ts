/** The rows a table view shows while edits are pending (task 7.8): the page with pending
 * values applied, deleted rows marked, and inserted rows after it. Pure. */
import type { KeyArg, Value } from "../../ipc";
import { type Pending, rowId } from "./pending";

/** What an inserted row shows for a column it leaves to the database default. */
export const DEFAULT_VALUE: Value = { t: "unknown", type: "default", text: "DEFAULT" };

export type DisplayRow = {
  values: Value[];
  /** `RowId` for page rows, the temporary id for inserted rows. */
  id: string;
  kind: "existing" | "inserted";
  deleted: boolean;
  /** Columns with a pending value. */
  edited: string[];
  /** Values before the pending edits, by column. */
  originals: Record<string, Value>;
  /** The row key's original values; `null` for inserted rows and rows without a key. */
  key: KeyArg[] | null;
};

export function displayRows(
  page: Value[][],
  names: string[],
  rowKey: string[] | null,
  pending: Pending,
): DisplayRow[] {
  const existing = page.map((values): DisplayRow => {
    const key = rowKey?.every((c) => names.includes(c))
      ? rowKey.map((column) => ({ column, value: values[names.indexOf(column)] ?? { t: "null" } }))
      : null;
    const id = key ? rowId(key) : "";
    const update = key ? pending.updates[id] : undefined;
    return {
      values: update
        ? names.map((name, i) => update.set[name] ?? values[i] ?? { t: "null" })
        : values,
      id,
      kind: "existing",
      deleted: key !== null && pending.deletes[id] !== undefined,
      edited: update ? Object.keys(update.set) : [],
      originals: update?.original ?? {},
      key,
    };
  });
  const inserted = pending.inserts.map((insert): DisplayRow => ({
    values: names.map((name) => insert.values[name] ?? DEFAULT_VALUE),
    id: insert.tempId,
    kind: "inserted",
    deleted: false,
    edited: Object.keys(insert.values),
    originals: {},
    key: null,
  }));
  return [...existing, ...inserted];
}
