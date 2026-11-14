import type { Namespace, ObjectSummary } from "../../ipc";
import {
  SESSION_KEY,
  type SessionTree,
  groupNodeKey,
  namespaceKey,
  namespaceNodeKey,
} from "./schemaStore";
import { type TreeRow, visibleRows } from "./tree";

const main = { catalog: null, schema: "main" };
const system: Namespace = {
  namespace: { catalog: null, schema: "temp" },
  name: "temp",
  system: true,
};
const objects: ObjectSummary[] = [
  { name: "customers", kind: "table", comment: null },
  { name: "orders", kind: "table", comment: null },
  { name: "paid_orders", kind: "view", comment: null },
  { name: "totals", kind: "materialized_view", comment: null },
];

function tree(overrides: Partial<SessionTree> = {}): SessionTree {
  return {
    namespaces: {
      status: "loaded",
      data: [{ namespace: main, name: "main", system: false }, system],
    },
    objects: { [namespaceKey(main)]: { status: "loaded", data: objects } },
    expanded: { [SESSION_KEY]: true, [namespaceNodeKey(main)]: true },
    ...overrides,
  };
}

function labels(rows: TreeRow[]): string[] {
  return rows.map((row) => {
    const indent = "  ".repeat(row.depth);
    switch (row.kind) {
      case "object":
        return `${indent}${row.object.name}`;
      case "group":
        return `${indent}${row.label} (${row.count})`;
      case "error":
        return `${indent}! ${row.error.message}`;
      case "loading":
        return `${indent}…`;
      default:
        return `${indent}${row.label}`;
    }
  });
}

const rows = (t: SessionTree, filter = "", showSystem = false) =>
  labels(visibleRows([{ sessionId: "s1", label: "Local", tree: t }], { filter, showSystem }));

describe("visibleRows", () => {
  it("nests session, namespace, groups and objects, hiding system namespaces", () => {
    expect(rows(tree())).toEqual([
      "Local",
      "  main",
      "    Tables (2)",
      "      customers",
      "      orders",
      "    Views (2)",
      "      paid_orders",
      "      totals",
    ]);
  });

  it("shows system namespaces when asked", () => {
    expect(rows(tree(), "", true)).toContain("  temp");
  });

  it("filters objects by name and drops empty groups", () => {
    expect(rows(tree(), "ORD")).toEqual([
      "Local",
      "  main",
      "    Tables (1)",
      "      orders",
      "    Views (1)",
      "      paid_orders",
    ]);
    expect(rows(tree(), "zzz")).toEqual(["Local", "  main", "    No matches"]);
  });

  it("hides the children of collapsed nodes", () => {
    const collapsed = tree({
      expanded: {
        [SESSION_KEY]: true,
        [namespaceNodeKey(main)]: true,
        [groupNodeKey(main, "tables")]: false,
      },
    });

    expect(rows(collapsed)).toEqual([
      "Local",
      "  main",
      "    Tables (2)",
      "    Views (2)",
      "      paid_orders",
      "      totals",
    ]);
    expect(rows(tree({ expanded: {} }))).toEqual(["Local"]);
  });

  it("shows loading and error states on the node that is loading", () => {
    const loading = tree({ objects: { [namespaceKey(main)]: { status: "loading" } } });
    const denied = tree({
      objects: {
        [namespaceKey(main)]: {
          status: "failed",
          error: { code: "permission", message: "denied" },
        },
      },
    });

    expect(rows(loading)).toEqual(["Local", "  main", "    …"]);
    expect(rows(denied)).toEqual(["Local", "  main", "    ! denied"]);
    expect(rows(tree({ namespaces: { status: "loading" } }))).toEqual(["Local", "  …"]);
  });

  it("flattens 2000 tables quickly", () => {
    const many = Array.from({ length: 2000 }, (_, i): ObjectSummary => ({
      name: `table_${i}`,
      kind: "table",
      comment: null,
    }));
    const big = tree({ objects: { [namespaceKey(main)]: { status: "loaded", data: many } } });

    const started = performance.now();
    const result = visibleRows([{ sessionId: "s1", label: "Local", tree: big }], {
      filter: "",
      showSystem: false,
    });

    expect(result).toHaveLength(2003);
    expect(performance.now() - started).toBeLessThan(100);
  });
});
