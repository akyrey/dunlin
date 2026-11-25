import type { TableInfo } from "../../ipc";
import { pendingViews, useEdits } from "../edit/editStore";
import { useSchemaEdits } from "./schemaEditStore";

const info: TableInfo = {
  table: { namespace: { catalog: null, schema: "main" }, name: "t" },
  kind: "table",
  comment: null,
  columns: [],
  primaryKey: null,
  indexes: [],
  foreignKeys: [],
  uniqueConstraints: [],
  checkConstraints: [],
  rowKey: null,
  rowId: null,
};

beforeEach(() => {
  useSchemaEdits.setState({ pending: {}, sessions: {} });
  useEdits.setState({ pending: {}, previewSkipped: {} });
});

describe("schema edit store", () => {
  it("reports staged changes to the guards and forgets them when cleared", () => {
    useSchemaEdits.getState().dispatch("v1", "s1", info, { type: "renameTable", name: "u" });
    expect(pendingViews(useEdits.getState(), "s1")).toEqual(["v1"]);
    useSchemaEdits.getState().clear("v1", "s1");
    expect(pendingViews(useEdits.getState(), "s1")).toEqual([]);
    expect(useSchemaEdits.getState().pending).toEqual({});
  });

  it("drops one session's changes and keeps the others'", () => {
    const { dispatch } = useSchemaEdits.getState();
    dispatch("v1", "s1", info, { type: "renameTable", name: "a" });
    dispatch("v2", "s2", info, { type: "renameTable", name: "b" });
    useSchemaEdits.getState().dropSession("s1");
    expect(Object.keys(useSchemaEdits.getState().pending)).toEqual(["v2"]);
    expect(pendingViews(useEdits.getState())).toEqual(["v2"]);
  });
});
