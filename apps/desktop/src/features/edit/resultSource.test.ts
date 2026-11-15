import type { TableInfo } from "../../ipc";
import { resultEditReason } from "./resultSource";

const info = {
  table: { namespace: { catalog: null, schema: "main" }, name: "people" },
  columns: [{ name: "id" }, { name: "name" }, { name: "balance" }],
  rowKey: ["id"],
} as unknown as TableInfo;

describe("resultEditReason", () => {
  it("allows the table's own columns including the row key", () => {
    expect(resultEditReason(info, ["name", "id"])).toBeNull();
  });

  it("explains a missing key, a foreign column and a repeated one", () => {
    expect(resultEditReason(info, ["name"])).toMatch(/select id too/);
    expect(resultEditReason(info, ["id", "nick"])).toMatch(/nick is not a column of people/);
    expect(resultEditReason(info, ["id", "id"])).toMatch(/more than once/);
  });
});
