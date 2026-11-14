import type { Value } from "../../ipc";
import { toTsv } from "./copy";
import { present } from "./present";
import { isSelected, navigate, rangeOf, select, selectAll, selectRow } from "./selection";
import { MAX_AUTO_WIDTH, MIN_WIDTH, columnWidth, widthFor } from "./widths";

const bounds = { rows: 10, cols: 5, pageRows: 3 };
const at = (row: number, col: number) => ({ anchor: { row, col }, focus: { row, col } });
const key = (k: string, shiftKey = false, mod = false) => ({ key: k, shiftKey, mod });

describe("selection", () => {
  it("moves with arrows, Home/End, PageUp/Down and clamps at the edges", () => {
    expect(navigate(at(0, 0), key("ArrowDown"), bounds)?.focus).toEqual({ row: 1, col: 0 });
    expect(navigate(at(0, 0), key("ArrowUp"), bounds)?.focus).toEqual({ row: 0, col: 0 });
    expect(navigate(at(2, 2), key("ArrowRight", false, true), bounds)?.focus).toEqual({
      row: 2,
      col: 4,
    });
    expect(navigate(at(2, 2), key("ArrowLeft", false, true), bounds)?.focus).toEqual({
      row: 2,
      col: 0,
    });
    expect(navigate(at(2, 2), key("ArrowDown", false, true), bounds)?.focus).toEqual({
      row: 9,
      col: 2,
    });
    expect(navigate(at(2, 2), key("ArrowUp", false, true), bounds)?.focus).toEqual({
      row: 0,
      col: 2,
    });
    expect(navigate(at(2, 2), key("Home"), bounds)?.focus).toEqual({ row: 2, col: 0 });
    expect(navigate(at(2, 2), key("End", false, true), bounds)?.focus).toEqual({ row: 9, col: 4 });
    expect(navigate(at(2, 2), key("Home", false, true), bounds)?.focus).toEqual({ row: 0, col: 0 });
    expect(navigate(at(2, 2), key("End"), bounds)?.focus).toEqual({ row: 2, col: 4 });
    expect(navigate(at(8, 1), key("PageDown"), bounds)?.focus).toEqual({ row: 9, col: 1 });
    expect(navigate(at(8, 1), key("PageUp"), bounds)?.focus).toEqual({ row: 5, col: 1 });
    expect(navigate(null, key("ArrowRight"), bounds)?.focus).toEqual({ row: 0, col: 1 });
    expect(navigate(at(0, 0), key("x"), bounds)).toBeUndefined();
    expect(navigate(null, key("ArrowDown"), { ...bounds, rows: 0 })).toBeUndefined();
  });

  it("extends a range with shift and selects rows and everything", () => {
    const extended =
      navigate(
        navigate(at(1, 1), key("ArrowDown", true), bounds) ?? null,
        key("ArrowRight", true),
        bounds,
      ) ?? null;
    const range = rangeOf(extended);

    expect(range).toEqual({ top: 1, bottom: 2, left: 1, right: 2 });
    expect(isSelected(range, 2, 2)).toBe(true);
    expect(isSelected(range, 3, 2)).toBe(false);
    expect(rangeOf(select({ row: 0, col: 0 }, true, extended))).toEqual({
      top: 0,
      bottom: 1,
      left: 0,
      right: 1,
    });
    expect(rangeOf(selectRow(4, 5, false, null))).toEqual({ top: 4, bottom: 4, left: 0, right: 4 });
    expect(rangeOf(selectRow(6, 5, true, at(4, 2)))).toEqual({
      top: 4,
      bottom: 6,
      left: 0,
      right: 4,
    });
    expect(rangeOf(selectAll(bounds))).toEqual({ top: 0, bottom: 9, left: 0, right: 4 });
    expect(selectAll({ ...bounds, rows: 0 })).toBeNull();
    expect(rangeOf(null)).toBeNull();
  });
});

describe("copy as TSV", () => {
  it("copies the range, quoting fields that need it and leaving NULL empty", () => {
    const rows: Value[][] = [
      [{ t: "int", v: "1" }, { t: "text", v: "a\tb" }, { t: "null" }],
      [
        { t: "int", v: "2" },
        { t: "text", v: 'say "hi"' },
        { t: "bytes", v: "3q0=", len: 2 },
      ],
    ];

    expect(toTsv(rows, { top: 0, bottom: 1, left: 0, right: 2 }, ["id", "text", "blob"])).toBe(
      'id\ttext\tblob\n1\t"a\tb"\t\n2\t"say ""hi"""\t0xdead',
    );
    expect(toTsv(rows, { top: 1, bottom: 1, left: 0, right: 0 })).toBe("2");
  });
});

describe("column widths", () => {
  it("fits the longest text within bounds", () => {
    expect(widthFor([])).toBe(MIN_WIDTH);
    expect(widthFor(["x".repeat(1000)])).toBe(MAX_AUTO_WIDTH);
    const rows: Value[][] = Array.from({ length: 60 }, (_, i) => [
      { t: "text", v: i === 59 ? "x".repeat(40) : "x" },
    ]);
    expect(columnWidth("c", rows, 0)).toBeLessThan(columnWidth("c", rows, 0, true));
  });
});

describe("cell presentation", () => {
  it.each<[Value, string, string]>([
    [{ t: "null" }, "null", "NULL"],
    [{ t: "text", v: "" }, "empty", ""],
    [{ t: "text", v: "NULL" }, "text", "NULL"],
    [{ t: "bool", v: true }, "true", "true"],
    [{ t: "bool", v: false }, "false", "false"],
    [{ t: "int", v: "9007199254740993" }, "number", "9007199254740993"],
    [{ t: "bytes", v: "AAE=", len: 2_516_582 }, "binary", "<binary 2.4 MiB>"],
    [{ t: "json", v: '{\n  "a": 1\n}' }, "json", '{ "a": 1 }'],
    [{ t: "text", v: "first\nsecond" }, "text", "first…"],
    [{ t: "text", v: "cut", truncated: true }, "text", "cut…"],
    [{ t: "array", v: [{ t: "int", v: "1" }, { t: "null" }] }, "array", "[1, NULL]"],
    [{ t: "date", v: "2026-11-14" }, "text", "2026-11-14"],
  ])("%o is %s %s", (value, kind, text) => {
    const shown = present(value, "text");
    expect(shown.kind).toBe(kind);
    expect(shown.text).toBe(text);
  });

  it("right-aligns numbers and cuts long text", () => {
    expect(present({ t: "int", v: "1" }, "integer").alignRight).toBe(true);
    expect(present({ t: "null" }, "text", "∅")).toMatchObject({ kind: "null", text: "∅" });
    expect(present({ t: "uuid", v: "x" }, "decimal").alignRight).toBe(true);
    expect(present({ t: "text", v: "y".repeat(300) }, "text")).toMatchObject({
      cut: true,
      text: `${"y".repeat(200)}…`,
    });
  });
});
