import type { Value } from "../../ipc";
import { toTsv } from "./copy";
import { present } from "./present";
import { MAX_AUTO_WIDTH, MIN_WIDTH, columnWidth, widthFor } from "./widths";

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
