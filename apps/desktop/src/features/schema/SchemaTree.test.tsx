import { render, screen, within } from "@testing-library/react";

import type { ObjectSummary } from "../../ipc";
import { SESSION_KEY, namespaceKey, namespaceNodeKey, useSchema } from "./schemaStore";
import { SchemaTree, type TreeSessionInfo } from "./SchemaTree";

const main = { catalog: null, schema: "main" };

const session = (
  sessionId: string,
  label: string,
  driver: TreeSessionInfo["driver"] = "sqlite",
  color: string | null = null,
): TreeSessionInfo => ({ connectionId: `c-${sessionId}`, sessionId, label, driver, color });

const loadedTree = (name: string) => ({
  namespaces: {
    status: "loaded" as const,
    data: [{ namespace: main, name: "main", system: false }],
  },
  objects: {
    [namespaceKey(main)]: {
      status: "loaded" as const,
      data: [{ name, kind: "table" as const, comment: null }],
    },
  },
  expanded: { [SESSION_KEY]: true, [namespaceNodeKey(main)]: true },
});

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(250);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("SchemaTree", () => {
  it("renders a 2000-table schema quickly and only the visible rows", () => {
    const many = Array.from({ length: 2000 }, (_, i): ObjectSummary => ({
      name: `table_${String(i).padStart(4, "0")}`,
      kind: "table",
      comment: null,
    }));
    useSchema.setState({
      filter: "",
      showSystem: false,
      trees: {
        s1: {
          namespaces: {
            status: "loaded",
            data: [{ namespace: main, name: "main", system: false }],
          },
          objects: { [namespaceKey(main)]: { status: "loaded", data: many } },
          expanded: { [SESSION_KEY]: true, [namespaceNodeKey(main)]: true },
        },
      },
    });

    const started = performance.now();
    render(<SchemaTree sessions={[session("s1", "Big")]} />);
    const elapsed = performance.now() - started;

    expect(screen.getByRole("button", { name: "table_0000" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "table_1999" })).toBeNull();
    expect(screen.getAllByRole("treeitem").length).toBeLessThan(80);
    expect(elapsed).toBeLessThan(1000);
  });

  it("marks each session with its database logo and colour", () => {
    useSchema.setState({
      filter: "",
      showSystem: false,
      trees: { s1: loadedTree("users"), s2: loadedTree("orders") },
    });

    render(
      <SchemaTree
        sessions={[session("s1", "Prod", "postgres", "#ef4444"), session("s2", "Local", "mysql")]}
      />,
    );

    const prod = screen.getByRole("button", { name: /Prod/ });
    expect(within(prod).getByRole("img", { name: "PostgreSQL" })).toBeInTheDocument();
    const local = screen.getByRole("button", { name: /Local/ });
    expect(within(local).getByRole("img", { name: "MySQL / MariaDB" })).toBeInTheDocument();
  });

  it("edges every row of a coloured session in its colour", () => {
    useSchema.setState({ filter: "", showSystem: false, trees: { s1: loadedTree("users") } });

    render(<SchemaTree sessions={[session("s1", "Prod", "postgres", "#ef4444")]} />);

    const table = screen.getByRole("button", { name: "users" }).closest("[data-session-row]");
    expect(table).toHaveStyle({ boxShadow: "inset 2px 0 0 #ef4444" });
  });

  it("renders nothing without open sessions", () => {
    const { container } = render(<SchemaTree sessions={[]} />);

    expect(container).toBeEmptyDOMElement();
  });
});
