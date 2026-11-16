import type { KeySummary } from "../../ipc";
import { buildKeyTree, visibleRows } from "./keyTree";

const summary = (name: string): KeySummary => ({
  key: { name, id: btoa(name) },
  keyType: "string",
});

const labels = (keys: string[], expanded: string[] = []) =>
  visibleRows(buildKeyTree(keys.map(summary)), new Set(expanded)).map(
    (row) =>
      `${"  ".repeat(row.depth)}${row.kind === "folder" ? `${row.label}/ (${row.count})` : row.label}`,
  );

describe("buildKeyTree", () => {
  it("groups keys by the colon separator, folders first, both sorted", () => {
    expect(labels(["user:2", "plain", "user:1", "a:b:c"])).toEqual([
      "a/ (1)",
      "user/ (2)",
      "plain",
    ]);
  });

  it("shows the children of expanded folders only", () => {
    expect(labels(["a:b:c", "a:d", "a:b:e"], ["a:"])).toEqual(["a/ (3)", "  b/ (2)", "  d"]);
    expect(labels(["a:b:c", "a:d", "a:b:e"], ["a:", "a:b:"])).toEqual([
      "a/ (3)",
      "  b/ (2)",
      "    c",
      "    e",
      "  d",
    ]);
  });

  it("keeps a key and a folder of the same name apart", () => {
    expect(labels(["user", "user:1"], ["user:"])).toEqual(["user/ (1)", "  1", "user"]);
  });

  it("labels a key with an empty last segment by its full name", () => {
    expect(labels(["a:", "a::b"], ["a:", "a::"])).toEqual([
      "a/ (2)",
      "  (empty)/ (1)",
      "    b",
      "  a:",
    ]);
  });

  it("keeps the summary on key rows and the prefix on folder rows", () => {
    const rows = visibleRows(buildKeyTree([summary("x:y")]), new Set(["x:"]));
    expect(rows[0]).toMatchObject({ kind: "folder", path: "x:" });
    expect(rows[1]).toMatchObject({ kind: "key", summary: summary("x:y") });
  });
});
