/** Staged edits of one table view (task 7.6). A pure reducer over immutable state: nothing here
 * talks to the backend, and nothing reaches the database before Apply (D-005). */
import type { KeyArg, OperationArg, Value } from "../../ipc";

export type RowId = string;

export type PendingUpdate = {
  /** The row key's original values: how the row is found, even when a key column changes. */
  key: KeyArg[];
  set: Record<string, Value>;
  /** The values before the edit, for "show original" and revert. */
  original: Record<string, Value>;
};

export type PendingInsert = { tempId: string; values: Record<string, Value> };

export type Pending = {
  updates: Record<RowId, PendingUpdate>;
  deletes: Record<RowId, { key: KeyArg[] }>;
  inserts: PendingInsert[];
};

export const EMPTY: Pending = { updates: {}, deletes: {}, inserts: [] };

export type Action =
  | { type: "edit"; row: RowId; key: KeyArg[]; column: string; value: Value; original: Value }
  | { type: "editInsert"; tempId: string; column: string; value: Value }
  /** Back to the column default for an inserted row. */
  | { type: "clearInsert"; tempId: string; column: string }
  | { type: "insert"; tempId: string; values?: Record<string, Value> }
  | { type: "delete"; rows: { row: RowId; key: KeyArg[] }[] }
  | { type: "deleteInsert"; tempIds: string[] }
  | { type: "revertCell"; row: RowId; column: string }
  | { type: "revertRow"; row: RowId }
  | { type: "discard" };

/** A row's id: its key's original values, which never change while edits are pending. */
export function rowId(key: KeyArg[]): RowId {
  return JSON.stringify(key.map((k) => [k.column, k.value]));
}

export function sameValue(a: Value | undefined, b: Value | undefined): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function without<T>(record: Record<string, T>, key: string): Record<string, T> {
  return Object.fromEntries(Object.entries(record).filter(([k]) => k !== key));
}

function edit(state: Pending, action: Extract<Action, { type: "edit" }>): Pending {
  if (state.deletes[action.row]) return state;
  const current = state.updates[action.row];
  const original = current?.original[action.column] ?? action.original;
  const set = { ...current?.set, [action.column]: action.value };
  const originals = { ...current?.original, [action.column]: original };
  // Back to where it started: the cell is no longer edited.
  if (sameValue(action.value, original)) {
    const remaining = without(set, action.column);
    if (Object.keys(remaining).length === 0)
      return { ...state, updates: without(state.updates, action.row) };
    return {
      ...state,
      updates: {
        ...state.updates,
        [action.row]: {
          key: current?.key ?? action.key,
          set: remaining,
          original: without(originals, action.column),
        },
      },
    };
  }
  return {
    ...state,
    updates: {
      ...state.updates,
      [action.row]: { key: current?.key ?? action.key, set, original: originals },
    },
  };
}

function revertCell(state: Pending, row: RowId, column: string): Pending {
  const current = state.updates[row];
  if (!current) return state;
  const set = without(current.set, column);
  if (Object.keys(set).length === 0) return { ...state, updates: without(state.updates, row) };
  return {
    ...state,
    updates: {
      ...state.updates,
      [row]: { ...current, set, original: without(current.original, column) },
    },
  };
}

export function reduce(state: Pending, action: Action): Pending {
  switch (action.type) {
    case "edit":
      return edit(state, action);
    case "editInsert":
      return {
        ...state,
        inserts: state.inserts.map((i) =>
          i.tempId === action.tempId
            ? { ...i, values: { ...i.values, [action.column]: action.value } }
            : i,
        ),
      };
    case "clearInsert":
      return {
        ...state,
        inserts: state.inserts.map((i) =>
          i.tempId === action.tempId ? { ...i, values: without(i.values, action.column) } : i,
        ),
      };
    case "insert":
      return {
        ...state,
        inserts: [...state.inserts, { tempId: action.tempId, values: action.values ?? {} }],
      };
    case "delete": {
      // A deleted row's pending update is dropped: there is nothing left to update.
      let updates = state.updates;
      const deletes = { ...state.deletes };
      for (const { row, key } of action.rows) {
        updates = without(updates, row);
        deletes[row] = { key };
      }
      return { ...state, updates, deletes };
    }
    case "deleteInsert":
      return { ...state, inserts: state.inserts.filter((i) => !action.tempIds.includes(i.tempId)) };
    case "revertCell":
      return revertCell(state, action.row, action.column);
    case "revertRow":
      return {
        ...state,
        updates: without(state.updates, action.row),
        deletes: without(state.deletes, action.row),
      };
    case "discard":
      return EMPTY;
  }
}

export type Counts = { updates: number; inserts: number; deletes: number; total: number };

export function counts(state: Pending): Counts {
  const updates = Object.keys(state.updates).length;
  const deletes = Object.keys(state.deletes).length;
  const inserts = state.inserts.length;
  return { updates, inserts, deletes, total: updates + inserts + deletes };
}

/** Which pending row an operation came from, to mark it when Apply fails. */
export type Source = { kind: "update" | "delete"; row: RowId } | { kind: "insert"; tempId: string };

const pairs = (record: Record<string, Value>): KeyArg[] =>
  Object.entries(record).map(([column, value]) => ({ column, value }));

/** The operations for `changes_preview`/`changes_apply`, and where each came from. */
export function toOperations(state: Pending): { operations: OperationArg[]; sources: Source[] } {
  const operations: OperationArg[] = [];
  const sources: Source[] = [];
  for (const [row, { key }] of Object.entries(state.deletes)) {
    operations.push({ kind: "delete", key });
    sources.push({ kind: "delete", row });
  }
  for (const [row, update] of Object.entries(state.updates)) {
    operations.push({ kind: "update", key: update.key, set: pairs(update.set) });
    sources.push({ kind: "update", row });
  }
  for (const insert of state.inserts) {
    operations.push({ kind: "insert", values: pairs(insert.values) });
    sources.push({ kind: "insert", tempId: insert.tempId });
  }
  return { operations, sources };
}
