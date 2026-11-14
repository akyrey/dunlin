import { CompletionContext } from "@codemirror/autocomplete";
import { schemaCompletionSource, sql } from "@codemirror/lang-sql";
import { EditorState } from "@codemirror/state";

import * as ipc from "../../ipc";
import { sessionFor, sqliteProfile } from "../../test/fixtures";
import { toNamespace, useCompletion } from "./completionStore";

vi.mock("../../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../ipc")>();
  return { ...actual, schemaNamespaces: vi.fn(), schemaCompletion: vi.fn() };
});

const session = sessionFor(sqliteProfile());

beforeEach(() => {
  useCompletion.setState({ completions: {}, defaults: {} });
  vi.mocked(ipc.schemaNamespaces).mockResolvedValue([
    { namespace: { catalog: null, schema: "main" }, name: "main", system: false },
    { namespace: { catalog: null, schema: "temp" }, name: "temp", system: true },
  ]);
  vi.mocked(ipc.schemaCompletion).mockResolvedValue({
    tables: [{ schema: "main", name: "users", columns: ["id", "email"] }],
  });
});

describe("completion store", () => {
  it("loads every non-system namespace with the session's default schema", async () => {
    await useCompletion.getState().load(session);

    expect(ipc.schemaCompletion).toHaveBeenCalledTimes(1);
    expect(useCompletion.getState().completions.s1).toEqual({
      schema: { main: { users: ["id", "email"] } },
      defaultSchema: "main",
      error: null,
    });
  });

  it("keeps keywords only, with the error, when loading fails; reloads only known sessions", async () => {
    vi.mocked(ipc.schemaNamespaces).mockRejectedValueOnce({ code: "permission", message: "no" });

    await useCompletion.getState().load(session);
    await useCompletion.getState().reload("unknown");

    expect(useCompletion.getState().completions.s1).toMatchObject({
      schema: {},
      error: { code: "permission" },
    });
    expect(useCompletion.getState().completions.unknown).toBeUndefined();
    await useCompletion.getState().reload("s1");
    expect(useCompletion.getState().completions.s1?.error).toBeNull();
  });

  it("forgets a dropped session, even mid-load", async () => {
    const loading = useCompletion.getState().load(session);
    useCompletion.getState().dropSession("s1");
    await loading;

    expect(useCompletion.getState().completions).toEqual({});
  });

  it("completes quickly on a schema of 2000 tables and 30 000 columns", async () => {
    const tables = Array.from({ length: 2000 }, (_, i) => ({
      schema: "public",
      name: `table_${i}`,
      columns: Array.from({ length: 15 }, (_, c) => `column_${c}`),
    }));
    const started = performance.now();
    const schema = toNamespace([{ tables }]);
    const source = schemaCompletionSource({ schema, defaultSchema: "public" });
    // Column completion after `table.` needs the SQL syntax tree.
    const state = (doc: string) => EditorState.create({ doc, extensions: [sql()] });
    const doc = "SELECT * FROM table_19";
    const result = await source(new CompletionContext(state(doc), doc.length, true));
    const columns = await source(new CompletionContext(state("SELECT table_7."), 15, true));

    expect(result?.options.length).toBeGreaterThanOrEqual(2000);
    expect(columns?.options.map((o) => o.label)).toContain("column_14");
    expect(performance.now() - started).toBeLessThan(500);
  });
});
