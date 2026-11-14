import type { Value } from "../../ipc";
import type { GridColumn } from "../grid/DataGrid";
import { editText, editorKind, formatHint, parseInput, toggled } from "./editors";
import { DEFAULT_VALUE, displayRows } from "./view";
import { EMPTY, reduce, rowId } from "./pending";

const col = (category: GridColumn["category"], typeName = ""): GridColumn => ({
  name: "c",
  typeName,
  category,
});
const nul: Value = { t: "null" };

describe("cell editors", () => {
  it.each<[GridColumn, Value, string]>([
    [col("binary"), nul, "binary"],
    [col("text"), { t: "bytes", v: "", len: 0 }, "binary"],
    [col("array"), nul, "none"],
    [col("text"), { t: "unknown", type: "hstore" }, "none"],
    [col("bool"), nul, "bool"],
    [col("json"), nul, "long"],
    [col("text"), { t: "text", v: "a\nb" }, "long"],
    [col("text"), { t: "text", v: "x".repeat(201) }, "long"],
    [col("text"), { t: "text", v: "cut", truncated: true }, "long"],
    [col("integer"), nul, "number"],
    [col("text"), { t: "decimal", v: "1" }, "number"],
    [col("temporal"), nul, "temporal"],
    [col("text"), { t: "date", v: "2026-01-01" }, "temporal"],
    [col("text"), { t: "text", v: "short" }, "text"],
  ])("%o with %o uses the %s editor", (column, value, kind) => {
    expect(editorKind(column, value)).toBe(kind);
  });

  it("keeps numbers as exact strings and validates them", () => {
    expect(parseInput(" 9007199254740993 ", col("integer"), nul)).toEqual({
      value: { t: "int", v: "9007199254740993" },
    });
    expect(parseInput("+5", col("integer", "INT UNSIGNED"), nul)).toEqual({
      value: { t: "uint", v: "5" },
    });
    expect(parseInput("-5", col("integer", "bigint unsigned"), nul)).toEqual({
      error: "Enter a number of 0 or more",
    });
    expect(parseInput("1.5", col("integer"), nul)).toEqual({ error: "Enter a whole number" });
    expect(parseInput("12345678901234567890.123456789", col("decimal"), nul)).toEqual({
      value: { t: "decimal", v: "12345678901234567890.123456789" },
    });
    expect(parseInput("1e3", col("float"), nul)).toEqual({ value: { t: "float", v: "1e3" } });
    expect(parseInput("abc", col("decimal"), nul)).toEqual({ error: "Enter a number" });
    expect(parseInput("7", col("text"), { t: "uint", v: "1" })).toEqual({
      value: { t: "uint", v: "7" },
    });
  });

  it("parses temporal values by the original's kind or the column type", () => {
    expect(parseInput("2026-11-14", col("temporal", "date"), nul)).toEqual({
      value: { t: "date", v: "2026-11-14" },
    });
    expect(parseInput("14/11/2026", col("temporal", "date"), nul)).toEqual({
      error: "Expected YYYY-MM-DD",
    });
    expect(parseInput("2026-11-14 10:00:00", col("temporal", "datetime"), nul)).toEqual({
      value: { t: "timestamp", v: "2026-11-14T10:00:00" },
    });
    expect(
      parseInput("2026-11-14 10:00+01:00", col("temporal", "timestamp with time zone"), nul),
    ).toEqual({
      value: { t: "timestamptz", v: "2026-11-14T10:00+01:00" },
    });
    expect(parseInput("10:00", col("temporal", "time"), nul)).toEqual({
      value: { t: "time", v: "10:00" },
    });
    expect(parseInput("2 days", col("temporal"), { t: "interval", v: "1 day" })).toEqual({
      value: { t: "interval", v: "2 days" },
    });
    expect(formatHint(col("temporal", "timestamptz"), nul)).toBe("YYYY-MM-DD HH:MM:SS+HH:MM");
    expect(formatHint(col("temporal", "interval"), nul)).toContain("day");
    expect(formatHint(col("temporal", "time"), nul)).toBe("HH:MM:SS");
  });

  it("validates JSON and XML, keeps long text, and refuses binary", () => {
    expect(parseInput('{"a": 1}', col("json"), nul)).toEqual({
      value: { t: "json", v: '{"a": 1}' },
    });
    expect(parseInput("{oops", col("json"), nul)).toEqual({ error: "Not valid JSON" });
    expect(parseInput("<a/>", col("xml"), nul)).toEqual({ value: { t: "xml", v: "<a/>" } });
    expect(parseInput("<a>", col("xml"), nul)).toEqual({ error: "Not well-formed XML" });
    expect(parseInput("a\nb", col("text"), { t: "text", v: "x\ny" })).toEqual({
      value: { t: "text", v: "a\nb" },
    });
    expect(parseInput("x", col("binary"), nul)).toEqual({
      error: "This value can only be set to NULL",
    });
  });

  it("treats an empty text as an empty string, and parses booleans", () => {
    expect(parseInput("", col("text"), { t: "text", v: "x" })).toEqual({
      value: { t: "text", v: "" },
    });
    expect(parseInput("x", col("uuid"), { t: "uuid", v: "y" })).toEqual({
      value: { t: "uuid", v: "x" },
    });
    expect(parseInput("Yes", col("bool"), nul)).toEqual({ value: { t: "bool", v: true } });
    expect(parseInput("0", col("bool"), nul)).toEqual({ value: { t: "bool", v: false } });
    expect(parseInput("maybe", col("bool"), nul)).toEqual({ error: "Enter true or false" });
    expect(toggled(nul)).toEqual({ t: "bool", v: true });
    expect(toggled({ t: "bool", v: true })).toEqual({ t: "bool", v: false });
    expect(editText(nul)).toBe("");
    expect(editText({ t: "int", v: "3" })).toBe("3");
  });
});

describe("display rows", () => {
  const names = ["id", "name"];
  const page: Value[][] = [
    [
      { t: "int", v: "1" },
      { t: "text", v: "Ada" },
    ],
    [
      { t: "int", v: "2" },
      { t: "text", v: "Grace" },
    ],
  ];
  const key = (id: string) => [{ column: "id", value: { t: "int" as const, v: id } }];

  it("applies pending values, marks deletes and appends inserts", () => {
    let pending = reduce(EMPTY, {
      type: "edit",
      row: rowId(key("1")),
      key: key("1"),
      column: "name",
      value: { t: "text", v: "Ada L." },
      original: { t: "text", v: "Ada" },
    });
    pending = reduce(pending, { type: "delete", rows: [{ row: rowId(key("2")), key: key("2") }] });
    pending = reduce(pending, {
      type: "insert",
      tempId: "t1",
      values: { name: { t: "text", v: "new" } },
    });

    const rows = displayRows(page, names, ["id"], pending);

    expect(rows.map((r) => [r.kind, r.deleted, r.edited])).toEqual([
      ["existing", false, ["name"]],
      ["existing", true, []],
      ["inserted", false, ["name"]],
    ]);
    expect(rows[0]?.values[1]).toEqual({ t: "text", v: "Ada L." });
    expect(rows[0]?.originals).toEqual({ name: { t: "text", v: "Ada" } });
    expect(rows[2]?.values).toEqual([DEFAULT_VALUE, { t: "text", v: "new" }]);
  });

  it("has no keys without a row key", () => {
    expect(displayRows(page, names, null, EMPTY).every((r) => r.key === null)).toBe(true);
    expect(displayRows(page, names, ["missing"], EMPTY)[0]?.key).toBeNull();
  });
});
