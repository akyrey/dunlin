import { render, screen, within } from "@testing-library/react";

import type { PlanNode, PlanReport, ReportedHint, TableInfo } from "../../ipc";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import userEvent from "@testing-library/user-event";
import { useEdits } from "../edit/editStore";
import { useSchemaEdits } from "../schema/schemaEditStore";
import { useWorkspace } from "../workspace/workspaceStore";
import { PlanView } from "./PlanView";

const step = (patch: Partial<PlanNode>): PlanNode => ({
  operation: "Seq Scan",
  relation: null,
  estimatedRows: null,
  details: [],
  cost: null,
  children: [],
  actualRows: null,
  loops: null,
  actualTimeMs: null,
  rowsRead: null,
  fullScan: false,
  table: null,
  filter: null,
  spilled: false,
  ...patch,
});

const estimated = (plan: PlanNode, hints: ReportedHint[] = []): PlanReport => ({
  plan,
  measured: false,
  totalMs: null,
  hints,
});

describe("PlanView", () => {
  it("shows each step with its estimates, details and share of the cost", () => {
    const plan = step({
      operation: "Hash Join",
      estimatedRows: 12_345.6,
      cost: 2000,
      details: ["Hash Cond: (o.customer_id = c.id)"],
      children: [
        step({ operation: "Seq Scan", relation: "orders", estimatedRows: 2.345, cost: 500 }),
        step({ operation: "Hash", cost: 4000 }),
      ],
    });

    render(
      <PlanView
        report={estimated(plan)}
        sql="SELECT * FROM orders"
        analyzed={false}
        sessionId="s1"
      />,
    );

    expect(screen.getByText("EXPLAIN SELECT * FROM orders")).toBeInTheDocument();
    const tree = screen.getByRole("tree", { name: "Query plan" });
    const root = within(tree).getAllByRole("treeitem")[0];
    expect(root).toHaveAttribute("aria-expanded", "true");
    expect(within(tree).getByText("rows ≈ 12,346")).toBeInTheDocument();
    expect(within(tree).getByText("rows ≈ 2.35")).toBeInTheDocument();
    expect(within(tree).getByText("cost 2,000")).toBeInTheDocument();
    expect(within(tree).getByText("Hash Cond: (o.customer_id = c.id)")).toBeInTheDocument();
    expect(within(tree).getByText("orders")).toBeInTheDocument();
    expect(within(tree).getByTitle("25% of the total cost")).toBeInTheDocument();
    // The root is the whole cost; a child above it (servers round differently) caps at 100%.
    expect(within(tree).getAllByTitle("100% of the total cost")).toHaveLength(2);
  });

  it("leaves out estimates and bars when the server reports no costs", () => {
    render(
      <PlanView
        report={estimated(step({ operation: "SCAN people" }))}
        sql="SELECT 1"
        analyzed={false}
        sessionId="s1"
      />,
    );

    const item = screen.getByRole("treeitem");
    expect(item).not.toHaveAttribute("aria-expanded");
    expect(item).toHaveTextContent("SCAN people");
    expect(screen.queryByText(/cost/)).toBeNull();
    expect(screen.queryByText(/rows ≈/)).toBeNull();
  });

  it("shows measured rows, loops and time, and the share of the total time", () => {
    const plan = step({
      operation: "Nested Loop",
      estimatedRows: 10,
      actualRows: 207,
      loops: 1,
      actualTimeMs: 4,
      children: [step({ operation: "Index Scan", actualRows: 1, loops: 207, actualTimeMs: 1 })],
    });
    render(
      <PlanView
        report={{ plan, measured: true, totalMs: 4.5, hints: [] }}
        sql="SELECT 1"
        analyzed
        sessionId="s1"
      />,
    );
    expect(screen.getByText(/EXPLAIN ANALYZE SELECT 1/)).toBeInTheDocument();
    expect(screen.getByText("4.5 ms in all")).toBeInTheDocument();
    expect(screen.getByText("rows 207")).toBeInTheDocument();
    expect(screen.getByText("rows 1 × 207")).toBeInTheDocument();
    expect(screen.getByTitle("25% of the total time")).toBeInTheDocument();
  });

  it("marks hints on their steps", () => {
    const plan = step({ operation: "Hash Join", children: [step({ operation: "Seq Scan" })] });
    const hints: ReportedHint[] = [
      {
        path: [0],
        kind: "hottest",
        message: "The most time is spent here: 80% of the total.",
        index: null,
      },
      {
        path: [0],
        kind: "full_scan",
        message: "Reads every row of orders (about 20000).",
        index: null,
      },
    ];
    render(
      <PlanView report={estimated(plan, hints)} sql="SELECT 1" analyzed={false} sessionId="s1" />,
    );
    const scan = screen.getAllByRole("treeitem").at(1);
    if (!scan) throw new Error("no scan step");
    expect(within(scan).getByText("slowest")).toBeInTheDocument();
    expect(within(scan).getByText("full scan")).toBeInTheDocument();
    expect(within(scan).getByLabelText(/Hint: Reads every row of orders/)).toBeInTheDocument();
    expect(within(scan).queryByRole("button", { name: /Create index/ })).toBeNull();
  });

  it("stages a suggested index in the table's detail tab", async () => {
    const table = { namespace: { catalog: null, schema: "main" }, name: "orders" };
    const info: TableInfo = {
      table,
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
    mockIPC((cmd) => (cmd === "schema_table" ? info : null));
    useSchemaEdits.setState({ pending: {}, sessions: {} });
    useEdits.setState({ pending: {}, previewSkipped: {} });
    useWorkspace.setState({ workspaces: {} });
    const index = {
      name: "orders_total_idx",
      columns: ["total"],
      unique: false,
      method: null,
      predicate: null,
    };
    const hints: ReportedHint[] = [
      {
        path: [],
        kind: "full_scan",
        message: "Reads every row of orders (about 20000).",
        index: { table, index },
      },
    ];
    render(
      <PlanView
        report={estimated(step({ operation: "Seq Scan" }), hints)}
        sql="SELECT 1"
        analyzed={false}
        sessionId="s1"
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "Create index on total…" }));

    await vi.waitFor(() => {
      expect(Object.values(useSchemaEdits.getState().pending)[0]?.addedObjects).toEqual([
        { kind: "index", value: index },
      ]);
    });
    expect(useWorkspace.getState().workspaces.s1?.views.some((v) => v.kind === "table")).toBe(true);
    expect(Object.keys(useEdits.getState().pending)).toHaveLength(1);
    clearMocks();
  });
});
