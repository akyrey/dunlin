import { render, screen } from "@testing-library/react";

import type { ObjectSummary } from "../../ipc";
import { SESSION_KEY, namespaceKey, namespaceNodeKey, useSchema } from "./schemaStore";
import { SchemaTree } from "./SchemaTree";

const main = { catalog: null, schema: "main" };

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
    render(<SchemaTree sessions={[{ connectionId: "c1", sessionId: "s1", label: "Big" }]} />);
    const elapsed = performance.now() - started;

    expect(screen.getByRole("button", { name: "table_0000" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "table_1999" })).toBeNull();
    expect(screen.getAllByRole("treeitem").length).toBeLessThan(80);
    expect(elapsed).toBeLessThan(1000);
  });

  it("renders nothing without open sessions", () => {
    const { container } = render(<SchemaTree sessions={[]} />);

    expect(container).toBeEmptyDOMElement();
  });
});
