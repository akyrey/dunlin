import type { TableRef } from "../../ipc";
import { EDITOR_VIEW, tableKey, useWorkspace, workspaceOf } from "./workspaceStore";

const table = (name: string): TableRef => ({ namespace: { catalog: null, schema: "main" }, name });
const current = () => workspaceOf(useWorkspace.getState(), "s1");

beforeEach(() => {
  useWorkspace.setState({ workspaces: {} });
});

describe("workspace store", () => {
  it("opens a table once and focuses it", () => {
    useWorkspace.getState().openTable("s1", table("a"));
    useWorkspace.getState().openTable("s1", table("b"));
    useWorkspace.getState().openTable("s1", table("a"));

    expect(current().details.map((d) => d.table.name)).toEqual(["a", "b"]);
    expect(current().active).toBe(tableKey(table("a")));
  });

  it("closing the active tab focuses its neighbour, then the editor", () => {
    for (const name of ["a", "b", "c"]) useWorkspace.getState().openTable("s1", table(name));
    useWorkspace.getState().activate("s1", tableKey(table("b")));

    useWorkspace.getState().close("s1", tableKey(table("b")));
    expect(current().active).toBe(tableKey(table("c")));
    useWorkspace.getState().close("s1", tableKey(table("c")));
    expect(current().active).toBe(tableKey(table("a")));
    useWorkspace.getState().close("s1", tableKey(table("a")));
    expect(current()).toEqual({ details: [], active: EDITOR_VIEW });
  });

  it("closing another tab keeps the active one, and unknown keys change nothing", () => {
    useWorkspace.getState().openTable("s1", table("a"));
    useWorkspace.getState().openTable("s1", table("b"));

    useWorkspace.getState().close("s1", tableKey(table("a")));
    useWorkspace.getState().close("s1", "nope");

    expect(current().active).toBe(tableKey(table("b")));
    expect(current().details).toHaveLength(1);
  });

  it("forgets a closed session", () => {
    useWorkspace.getState().openTable("s1", table("a"));

    useWorkspace.getState().dropSession("s1");

    expect(useWorkspace.getState().workspaces).toEqual({});
  });
});
