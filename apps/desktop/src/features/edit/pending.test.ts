import type { KeyArg, Value } from "../../ipc";
import { EMPTY, type Pending, counts, reduce, rowId, sameValue, toOperations } from "./pending";

const int = (v: number): Value => ({ t: "int", v: String(v) });
const text = (v: string): Value => ({ t: "text", v });
const key = (id: number): KeyArg[] => [{ column: "id", value: int(id) }];
const r1 = rowId(key(1));
const r2 = rowId(key(2));

const edit = (state: Pending, column: string, value: Value, original: Value, id = 1) =>
  reduce(state, { type: "edit", row: rowId(key(id)), key: key(id), column, value, original });

describe("pending changes", () => {
  it("stages an edit and keeps the first original value", () => {
    const once = edit(EMPTY, "name", text("B"), text("A"));
    const twice = edit(once, "name", text("C"), text("B"));

    expect(twice.updates[r1]).toEqual({
      key: key(1),
      set: { name: text("C") },
      original: { name: text("A") },
    });
    expect(counts(twice)).toEqual({ updates: 1, inserts: 0, deletes: 0, total: 1 });
  });

  it("removes the entry when a cell is edited back to its original value", () => {
    const two = edit(edit(EMPTY, "name", text("B"), text("A")), "note", text("x"), { t: "null" });

    const back = edit(two, "name", text("A"), text("B"));
    expect(back.updates[r1]?.set).toEqual({ note: text("x") });
    expect(back.updates[r1]?.original).toEqual({ note: { t: "null" } });

    const none = edit(back, "note", { t: "null" }, text("x"));
    expect(none.updates).toEqual({});
    expect(edit(EMPTY, "name", text("A"), text("A"))).toEqual(EMPTY);
  });

  it("addresses a row by its original key even after the key column changes", () => {
    const moved = edit(EMPTY, "id", int(5), int(1));
    const again = edit(moved, "name", text("n"), text("o"));

    expect(again.updates[r1]?.key).toEqual(key(1));
    expect(toOperations(again).operations).toEqual([
      {
        kind: "update",
        key: key(1),
        set: [
          { column: "id", value: int(5) },
          { column: "name", value: text("n") },
        ],
      },
    ]);
  });

  it("drops a deleted row's update, ignores edits to it, and reverts both", () => {
    const edited = edit(EMPTY, "name", text("B"), text("A"));
    const deleted = reduce(edited, {
      type: "delete",
      rows: [
        { row: r1, key: key(1) },
        { row: r2, key: key(2) },
      ],
    });

    expect(deleted.updates).toEqual({});
    expect(Object.keys(deleted.deletes)).toEqual([r1, r2]);
    expect(edit(deleted, "name", text("C"), text("A"))).toBe(deleted);
    expect(reduce(deleted, { type: "revertRow", row: r1 }).deletes).toEqual({
      [r2]: { key: key(2) },
    });
  });

  it("reverts one cell or a whole row", () => {
    const two = edit(edit(EMPTY, "name", text("B"), text("A")), "note", text("x"), text("y"));

    const one = reduce(two, { type: "revertCell", row: r1, column: "name" });
    expect(one.updates[r1]?.set).toEqual({ note: text("x") });
    expect(reduce(one, { type: "revertCell", row: r1, column: "note" }).updates).toEqual({});
    expect(reduce(two, { type: "revertRow", row: r1 })).toEqual(EMPTY);
    expect(reduce(EMPTY, { type: "revertCell", row: r1, column: "x" })).toBe(EMPTY);
  });

  it("adds, edits, clears and deletes inserted rows; deleting an insert removes it", () => {
    const added = reduce(reduce(EMPTY, { type: "insert", tempId: "a" }), {
      type: "insert",
      tempId: "b",
      values: { name: text("copy") },
    });
    const edited = reduce(added, {
      type: "editInsert",
      tempId: "a",
      column: "name",
      value: text("new"),
    });
    const cleared = reduce(edited, { type: "clearInsert", tempId: "b", column: "name" });

    expect(cleared.inserts).toEqual([
      { tempId: "a", values: { name: text("new") } },
      { tempId: "b", values: {} },
    ]);
    expect(
      reduce(cleared, { type: "deleteInsert", tempIds: ["a"] }).inserts.map((i) => i.tempId),
    ).toEqual(["b"]);
  });

  it("lists operations as deletes, updates, inserts with their sources, and discards", () => {
    let state = reduce(EMPTY, { type: "insert", tempId: "t", values: { name: text("n") } });
    state = edit(state, "name", text("B"), text("A"), 2);
    state = reduce(state, { type: "delete", rows: [{ row: r1, key: key(1) }] });

    const { operations, sources } = toOperations(state);

    expect(operations.map((o) => o.kind)).toEqual(["delete", "update", "insert"]);
    expect(sources).toEqual([
      { kind: "delete", row: r1 },
      { kind: "update", row: r2 },
      { kind: "insert", tempId: "t" },
    ]);
    expect(reduce(state, { type: "discard" })).toEqual(EMPTY);
    expect(counts(state).total).toBe(3);
  });

  it("compares values structurally", () => {
    expect(sameValue(text("a"), text("a"))).toBe(true);
    expect(sameValue(text(""), { t: "null" })).toBe(false);
    expect(sameValue(int(1), text("1"))).toBe(false);
  });
});
