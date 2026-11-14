import type { ColumnMeta, TableInfo, Value } from "../../ipc";
import { loadFilters, rememberFilter } from "./filterHistory";
import { describeMatch, followTarget, gridColumns, nextSort, rowKeyOf } from "./model";

const main = { catalog: null, schema: "main" };
const info: TableInfo = {
  table: { namespace: main, name: "shipments" },
  kind: "table",
  comment: null,
  columns: [
    {
      name: "id",
      typeName: "INTEGER",
      category: "integer",
      nullable: false,
      default: null,
      comment: null,
      ordinal: 1,
      generated: false,
    },
    {
      name: "order_id",
      typeName: "INTEGER",
      category: "integer",
      nullable: false,
      default: null,
      comment: null,
      ordinal: 2,
      generated: false,
    },
    {
      name: "line",
      typeName: "INTEGER",
      category: "integer",
      nullable: false,
      default: null,
      comment: null,
      ordinal: 3,
      generated: false,
    },
  ],
  primaryKey: { name: null, columns: ["id"] },
  indexes: [],
  foreignKeys: [
    {
      name: null,
      columns: ["order_id", "line"],
      referencedTable: { namespace: main, name: "order_items" },
      referencedColumns: ["order_id", "line"],
      onUpdate: null,
      onDelete: null,
    },
  ],
  uniqueConstraints: [],
  checkConstraints: [],
  rowKey: ["id"],
};
const page: ColumnMeta[] = ["id", "order_id", "line"].map((name) => ({
  name,
  typeName: "",
  category: "other",
  nullable: null,
}));
const names = ["id", "order_id", "line"];
const row: Value[] = [
  { t: "int", v: "7" },
  { t: "int", v: "10" },
  { t: "int", v: "2" },
];

describe("table data model", () => {
  it("marks keys and uses the table's types", () => {
    expect(
      gridColumns(page, info).map((c) => [c.name, c.typeName, c.primaryKey, c.references]),
    ).toEqual([
      ["id", "INTEGER", true, undefined],
      ["order_id", "INTEGER", false, "main.order_items"],
      ["line", "INTEGER", false, "main.order_items"],
    ]);
    expect(gridColumns(page, null)[0]).toMatchObject({ typeName: "", primaryKey: false });
  });

  it("cycles sorts and adds secondary keys with shift", () => {
    expect(nextSort([], "a", false)).toEqual([{ column: "a", descending: false }]);
    expect(nextSort([{ column: "a", descending: false }], "a", false)).toEqual([
      { column: "a", descending: true },
    ]);
    expect(nextSort([{ column: "a", descending: true }], "a", false)).toEqual([]);
    expect(nextSort([{ column: "a", descending: false }], "b", false)).toEqual([
      { column: "b", descending: false },
    ]);
    const two = nextSort([{ column: "a", descending: false }], "b", true);
    expect(two).toEqual([
      { column: "a", descending: false },
      { column: "b", descending: false },
    ]);
    expect(nextSort(two, "b", true)).toEqual([
      { column: "a", descending: false },
      { column: "b", descending: true },
    ]);
    expect(nextSort(nextSort(two, "b", true), "b", true)).toEqual([
      { column: "a", descending: false },
    ]);
  });

  it("finds the row key and where a composite foreign key leads", () => {
    expect(rowKeyOf(info, names, row)).toEqual([{ column: "id", value: { t: "int", v: "7" } }]);
    expect(rowKeyOf({ ...info, rowKey: null }, names, row)).toBeNull();
    expect(followTarget(info, names, row, "line")).toEqual({
      table: { namespace: main, name: "order_items" },
      match: [
        { column: "order_id", value: { t: "int", v: "10" } },
        { column: "line", value: { t: "int", v: "2" } },
      ],
    });
    expect(followTarget(info, names, row, "id")).toBeNull();
    expect(
      followTarget(info, names, [row[0] as Value, { t: "null" }, row[2] as Value], "line"),
    ).toBeNull();
    expect(
      describeMatch([
        { column: "id", value: { t: "int", v: "1" } },
        { column: "x", value: { t: "null" } },
      ]),
    ).toBe("id = 1, x = NULL");
  });

  it("remembers the ten latest distinct filters", () => {
    for (let i = 0; i < 12; i++) rememberFilter("t", `a > ${i}`);
    rememberFilter("t", "a > 5");
    rememberFilter("t", "   ");
    localStorage.setItem("dunlin.filters.bad", "{");

    expect(loadFilters("t")).toHaveLength(10);
    expect(loadFilters("t")[0]).toBe("a > 5");
    expect(loadFilters("bad")).toEqual([]);
  });
});
