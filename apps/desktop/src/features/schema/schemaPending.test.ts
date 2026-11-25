import { describe, expect, it } from "vitest";

import type { Check, Column, ColumnDefinition, Key, TableIndex, TableInfo } from "../../ipc";
import {
  EMPTY,
  type SchemaAction,
  type SchemaPending,
  changeCount,
  definitionOf,
  reduce,
  toChanges,
} from "./schemaPending";

function column(name: string, ordinal: number, nullable = true): Column {
  return {
    name,
    typeName: "integer",
    category: "integer",
    nullable,
    default: null,
    comment: null,
    ordinal,
    generated: false,
  };
}

const info: TableInfo = {
  table: { namespace: { catalog: null, schema: "public" }, name: "orders" },
  kind: "table",
  comment: null,
  columns: [column("id", 1, false), column("total", 2), column("note", 3)],
  primaryKey: { name: "orders_pkey", columns: ["id"] },
  indexes: [
    { name: "orders_total_idx", columns: ["total"], unique: false, method: null, predicate: null },
  ],
  foreignKeys: [],
  uniqueConstraints: [{ name: "orders_note_key", columns: ["note"] }],
  checkConstraints: [{ name: "orders_total_check", expression: "total >= 0" }],
  rowKey: ["id"],
  rowId: null,
};

const totalColumn = column("total", 2);
const totalIndex: TableIndex = {
  name: "orders_total_idx",
  columns: ["total"],
  unique: false,
  method: null,
  predicate: null,
};
const primaryKey: Key = { name: "orders_pkey", columns: ["id"] };
const noteKey: Key = { name: "orders_note_key", columns: ["note"] };
const totalCheck: Check = { name: "orders_total_check", expression: "total >= 0" };

const def = (name: string, typeName = "integer"): ColumnDefinition => ({
  name,
  typeName,
  nullable: true,
  default: null,
  comment: null,
});

const step = (state: SchemaPending, action: SchemaAction): SchemaPending =>
  reduce(state, action, info);

function apply(...actions: SchemaAction[]): SchemaPending {
  return actions.reduce(step, EMPTY);
}

describe("schema pending changes", () => {
  it("starts empty", () => {
    expect(changeCount(EMPTY)).toBe(0);
    expect(toChanges(info, EMPTY)).toEqual([]);
  });

  it("alters a column and forgets an alter back to the original", () => {
    const altered = apply({
      type: "alterColumn",
      name: "total",
      column: { ...def("amount"), nullable: false },
    });
    expect(toChanges(info, altered)).toEqual([
      {
        kind: "alter_column",
        name: "total",
        column: { ...def("amount"), nullable: false },
        using: null,
      },
    ]);
    const back = step(altered, {
      type: "alterColumn",
      name: "total",
      column: definitionOf(totalColumn),
    });
    expect(changeCount(back)).toBe(0);
  });

  it("keeps a USING expression only with the alter", () => {
    const state = apply({
      type: "alterColumn",
      name: "total",
      column: def("total", "bigint"),
      using: "total::bigint",
    });
    expect(toChanges(info, state)).toEqual([
      {
        kind: "alter_column",
        name: "total",
        column: def("total", "bigint"),
        using: "total::bigint",
      },
    ]);
  });

  it("drops a column, forgetting its alter, and restores it", () => {
    const dropped = apply(
      { type: "alterColumn", name: "note", column: def("remark") },
      { type: "dropColumn", name: "note" },
    );
    expect(toChanges(info, dropped)).toEqual([{ kind: "drop_column", name: "note" }]);
    const restored = step(dropped, { type: "restoreColumn", name: "note" });
    expect(changeCount(restored)).toBe(0);
  });

  it("adds, edits and removes new columns", () => {
    const added = apply(
      { type: "addColumn", tempId: "a", column: def("shipped") },
      { type: "addColumn", tempId: "b", column: def("weight") },
      { type: "editAdded", tempId: "a", column: def("shipped_at", "timestamp") },
    );
    expect(toChanges(info, added)).toEqual([
      { kind: "add_column", column: def("shipped_at", "timestamp") },
      { kind: "add_column", column: def("weight") },
    ]);
    const removed = step(added, { type: "removeAdded", tempId: "b" });
    expect(changeCount(removed)).toBe(1);
  });

  it("drops and restores keys, checks and indexes by value", () => {
    const state = apply(
      { type: "dropObject", object: { kind: "index", value: totalIndex } },
      { type: "dropObject", object: { kind: "check", value: totalCheck } },
      { type: "dropObject", object: { kind: "primaryKey", value: primaryKey } },
    );
    expect(changeCount(state)).toBe(3);
    const restored = step(state, {
      type: "restoreObject",
      object: { kind: "check", value: totalCheck },
    });
    expect(toChanges(info, restored)).toEqual([
      { kind: "drop_primary_key" },
      { kind: "drop_index", index: info.indexes[0] },
    ]);
  });

  it("dropping the same object twice stages it once", () => {
    const object = { kind: "unique", value: noteKey } as const;
    const state = apply({ type: "dropObject", object }, { type: "dropObject", object });
    expect(changeCount(state)).toBe(1);
  });

  it("adds objects and removes an added one", () => {
    const state = apply(
      { type: "addObject", object: { kind: "unique", value: { name: null, columns: ["total"] } } },
      {
        type: "addObject",
        object: { kind: "check", value: { name: "positive", expression: "total > 0" } },
      },
    );
    const removed = step(state, { type: "removeObject", index: 0 });
    expect(toChanges(info, removed)).toEqual([
      { kind: "add_check", check: { name: "positive", expression: "total > 0" } },
    ]);
  });

  it("orders drops, column changes, additions, then the table rename", () => {
    const state = apply(
      { type: "renameTable", name: "purchases" },
      {
        type: "addObject",
        object: {
          kind: "index",
          value: { name: "i", columns: ["amount"], unique: false, method: null, predicate: null },
        },
      },
      { type: "addColumn", tempId: "a", column: def("shipped") },
      { type: "alterColumn", name: "total", column: def("amount") },
      { type: "dropObject", object: { kind: "index", value: totalIndex } },
      { type: "setComment", comment: "All orders" },
    );
    expect(toChanges(info, state).map((c) => c.kind)).toEqual([
      "drop_index",
      "alter_column",
      "add_column",
      "add_index",
      "set_comment",
      "rename_table",
    ]);
  });

  it("a comment or name back to the original is no change", () => {
    const state = apply(
      { type: "setComment", comment: "x" },
      { type: "setComment", comment: null },
      { type: "renameTable", name: "x" },
      { type: "renameTable", name: "orders" },
    );
    expect(changeCount(state)).toBe(0);
  });

  it("an empty comment clears it", () => {
    const commented = { ...info, comment: "old" };
    const state = reduce(EMPTY, { type: "setComment", comment: "  " }, commented);
    expect(toChanges(commented, state)).toEqual([{ kind: "set_comment", comment: null }]);
  });

  it("discards everything", () => {
    const state = apply(
      { type: "dropColumn", name: "note" },
      { type: "addColumn", tempId: "a", column: def("x") },
      { type: "discard" },
    );
    expect(state).toEqual(EMPTY);
  });

  it("never mutates the state it is given", () => {
    const before = apply({ type: "addColumn", tempId: "a", column: def("x") });
    const frozen = JSON.stringify(before);
    step(before, { type: "dropColumn", name: "note" });
    step(before, { type: "removeAdded", tempId: "a" });
    expect(JSON.stringify(before)).toBe(frozen);
  });
});
