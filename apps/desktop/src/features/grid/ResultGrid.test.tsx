import { render, screen, within } from "@testing-library/react";

import type { ColumnMeta, Value } from "../../ipc";
import { ResultGrid } from "./ResultGrid";

const columns: ColumnMeta[] = [
  { name: "id", typeName: "INTEGER", category: "integer", nullable: null },
  { name: "name", typeName: "TEXT", category: "text", nullable: null },
];

function rows(count: number): Value[][] {
  return Array.from({ length: count }, (_, i) => [
    { t: "int", v: String(i + 1) },
    i === 0 ? { t: "null" } : { t: "text", v: `row ${i + 1}` },
  ]);
}

const VIEWPORT_HEIGHT = 300;

beforeEach(() => {
  // jsdom has no layout; TanStack Virtual sizes the viewport from clientWidth/clientHeight.
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(VIEWPORT_HEIGHT);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("ResultGrid", () => {
  it("renders headers and cells as text", () => {
    render(<ResultGrid columns={columns} rows={rows(2)} />);

    expect(screen.getAllByRole("columnheader").map((h) => h.textContent)).toEqual([
      "#",
      "id",
      "name",
    ]);
    expect(screen.getByText("row 2")).toBeInTheDocument();
  });

  it("shows NULL distinctly", () => {
    render(<ResultGrid columns={columns} rows={rows(1)} />);

    const nullCell = screen.getByText("NULL");
    expect(nullCell).toHaveClass("italic");
  });

  it("renders only the visible rows of a large result", () => {
    render(<ResultGrid columns={columns} rows={rows(10_000)} />);

    const grid = screen.getByRole("grid");
    expect(grid).toHaveAttribute("aria-rowcount", "10001");
    expect(within(grid).getAllByRole("row").length).toBeLessThan(60);
  });

  it("never interprets cell content as HTML", () => {
    const html: Value[][] = [
      [
        { t: "int", v: "1" },
        { t: "text", v: "<img src=x onerror=alert(1)>" },
      ],
    ];

    const { container } = render(<ResultGrid columns={columns} rows={html} />);

    expect(container.querySelector("img")).toBeNull();
    expect(screen.getByText("<img src=x onerror=alert(1)>")).toBeInTheDocument();
  });

  it("right-aligns numeric columns", () => {
    render(<ResultGrid columns={columns} rows={rows(2)} />);

    const [idCell, nameCell] = screen.getAllByRole("gridcell");
    expect(idCell).toHaveClass("text-right");
    expect(nameCell).not.toHaveClass("text-right");
  });
});
