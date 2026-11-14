import type { Namespace, ObjectSummary } from "../../ipc";
import * as ipc from "../../ipc";
import { SESSION_KEY, namespaceKey, namespaceNodeKey, useSchema } from "./schemaStore";

vi.mock("../../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../ipc")>();
  return {
    ...actual,
    schemaNamespaces: vi.fn(),
    schemaObjects: vi.fn(),
    schemaRefresh: vi.fn(),
  };
});

const ns = (schema: string, system = false): Namespace => ({
  namespace: { catalog: null, schema },
  name: schema,
  system,
});
const tables: ObjectSummary[] = [{ name: "t", kind: "table", comment: null }];

beforeEach(() => {
  useSchema.setState({ trees: {}, filter: "", showSystem: false });
  vi.mocked(ipc.schemaNamespaces).mockReset();
  vi.mocked(ipc.schemaObjects).mockReset();
  vi.mocked(ipc.schemaRefresh).mockReset();
});

const tree = () => useSchema.getState().trees.s1;

describe("schema store", () => {
  it("loads namespaces on first expand and opens a lone namespace", async () => {
    vi.mocked(ipc.schemaNamespaces).mockResolvedValue([ns("main"), ns("temp", true)]);
    vi.mocked(ipc.schemaObjects).mockResolvedValue(tables);

    await useSchema.getState().toggle("s1", SESSION_KEY);

    expect(tree()?.namespaces).toEqual({ status: "loaded", data: [ns("main"), ns("temp", true)] });
    expect(tree()?.expanded[namespaceNodeKey(ns("main").namespace)]).toBe(true);
    expect(tree()?.objects[namespaceKey(ns("main").namespace)]).toEqual({
      status: "loaded",
      data: tables,
    });
  });

  it("loads a namespace's objects once, and records a failure on that node", async () => {
    vi.mocked(ipc.schemaNamespaces).mockResolvedValue([ns("a"), ns("b")]);
    vi.mocked(ipc.schemaObjects).mockRejectedValue({ code: "permission", message: "denied" });
    const a = ns("a").namespace;

    await useSchema.getState().toggle("s1", SESSION_KEY);
    await useSchema.getState().toggle("s1", namespaceNodeKey(a), a);
    await useSchema.getState().toggle("s1", namespaceNodeKey(a), a);
    await useSchema.getState().toggle("s1", namespaceNodeKey(a), a);

    expect(ipc.schemaObjects).toHaveBeenCalledTimes(1);
    expect(tree()?.objects[namespaceKey(a)]).toEqual({
      status: "failed",
      error: { code: "permission", message: "denied" },
    });
    expect(tree()?.expanded[namespaceNodeKey(a)]).toBe(true);
  });

  it("refresh clears the backend cache and reloads what was loaded", async () => {
    vi.mocked(ipc.schemaNamespaces).mockResolvedValue([ns("main")]);
    vi.mocked(ipc.schemaObjects).mockResolvedValue(tables);
    await useSchema.getState().toggle("s1", SESSION_KEY);
    vi.mocked(ipc.schemaObjects).mockResolvedValue([
      ...tables,
      { name: "u", kind: "view", comment: null },
    ]);

    await useSchema.getState().refresh("s1");

    expect(ipc.schemaRefresh).toHaveBeenCalledWith("s1");
    expect(tree()?.objects[namespaceKey(ns("main").namespace)]).toMatchObject({
      data: [{ name: "t" }, { name: "u" }],
    });
  });

  it("a failed refresh is shown on the session", async () => {
    useSchema.setState({ trees: { s1: { objects: {}, expanded: {} } } });
    vi.mocked(ipc.schemaRefresh).mockRejectedValue({ code: "not_found", message: "gone" });

    await useSchema.getState().refresh("s1");

    expect(tree()?.namespaces).toMatchObject({ status: "failed" });
  });

  it("ignores results for a session dropped while loading", async () => {
    let resolve: (value: Namespace[]) => void = () => undefined;
    vi.mocked(ipc.schemaNamespaces).mockReturnValue(
      new Promise((r) => {
        resolve = r;
      }),
    );

    const loading = useSchema.getState().toggle("s1", SESSION_KEY);
    useSchema.getState().dropSession("s1");
    resolve([ns("main")]);
    await loading;

    expect(useSchema.getState().trees).toEqual({});
    expect(ipc.schemaObjects).not.toHaveBeenCalled();
  });
});
