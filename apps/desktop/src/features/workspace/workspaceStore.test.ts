import type { TableRef } from "../../ipc";
import { tableKey, useWorkspace, workspaceOf } from "./workspaceStore";

const table = (name: string): TableRef => ({ namespace: { catalog: null, schema: "main" }, name });
const current = () => workspaceOf(useWorkspace.getState(), "s1");
const keys = () => current().views.map((v) => v.key);

beforeEach(() => {
  useWorkspace.setState({ workspaces: {} });
});

describe("workspace store", () => {
  it("opens editors and tables once each and focuses them", () => {
    useWorkspace.getState().openEditor("s1", "t1");
    useWorkspace.getState().openTable("s1", table("a"));
    useWorkspace.getState().openTable("s1", table("a"));

    expect(keys()).toEqual(["t1", tableKey(table("a"))]);
    expect(current().active).toBe(tableKey(table("a")));
  });

  it("opens a table's data once per match", () => {
    const match = [{ column: "id", value: { t: "int" as const, v: "1" } }];
    useWorkspace.getState().openData("s1", table("a"));
    useWorkspace.getState().openData("s1", table("a"));
    useWorkspace.getState().openData("s1", table("a"), match);

    expect(current().views.map((v) => v.kind)).toEqual(["data", "data"]);
    expect(current().views[1]).toMatchObject({ table: table("a"), match });
  });

  it("closing the active view focuses its neighbour, then nothing", () => {
    for (const key of ["a", "b", "c"]) useWorkspace.getState().openEditor("s1", key);
    useWorkspace.getState().activate("s1", "b");

    useWorkspace.getState().close("s1", "b");
    expect(current().active).toBe("c");
    useWorkspace.getState().close("s1", "c");
    expect(current().active).toBe("a");
    useWorkspace.getState().close("s1", "a");
    expect(current()).toEqual({ views: [], active: null });
  });

  it("closing another view keeps the active one; unknown keys change nothing", () => {
    useWorkspace.getState().openEditor("s1", "a");
    useWorkspace.getState().openEditor("s1", "b");

    useWorkspace.getState().close("s1", "a");
    useWorkspace.getState().close("s1", "nope");
    useWorkspace.getState().activate("s1", "nope");

    expect(current()).toEqual({ views: [{ kind: "editor", key: "b" }], active: "b" });
  });

  it("cycles through views in both directions, wrapping around", () => {
    for (const key of ["a", "b", "c"]) useWorkspace.getState().openEditor("s1", key);

    useWorkspace.getState().cycle("s1", 1);
    expect(current().active).toBe("a");
    useWorkspace.getState().cycle("s1", -1);
    expect(current().active).toBe("c");
    useWorkspace.getState().cycle("s2", 1);
    expect(workspaceOf(useWorkspace.getState(), "s2").views).toEqual([]);
  });

  it("moves a view to another position", () => {
    for (const key of ["a", "b", "c"]) useWorkspace.getState().openEditor("s1", key);

    useWorkspace.getState().move("s1", "c", 0);
    expect(keys()).toEqual(["c", "a", "b"]);
    useWorkspace.getState().move("s1", "c", 99);
    expect(keys()).toEqual(["a", "b", "c"]);
    useWorkspace.getState().move("s1", "nope", 0);
    expect(keys()).toEqual(["a", "b", "c"]);
  });

  it("forgets a closed session", () => {
    useWorkspace.getState().openEditor("s1", "a");

    useWorkspace.getState().dropSession("s1");

    expect(useWorkspace.getState().workspaces).toEqual({});
  });
});
