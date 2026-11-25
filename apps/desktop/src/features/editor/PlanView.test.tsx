import { render, screen, within } from "@testing-library/react";

import type { PlanNode } from "../../ipc";
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

    render(<PlanView plan={plan} sql="SELECT * FROM orders" />);

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
    render(<PlanView plan={step({ operation: "SCAN people" })} sql="SELECT 1" />);

    const item = screen.getByRole("treeitem");
    expect(item).not.toHaveAttribute("aria-expanded");
    expect(item).toHaveTextContent("SCAN people");
    expect(screen.queryByText(/cost/)).toBeNull();
    expect(screen.queryByText(/rows ≈/)).toBeNull();
  });
});
