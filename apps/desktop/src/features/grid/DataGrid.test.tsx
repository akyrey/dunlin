import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Value } from "../../ipc";
import { clipboardWrite, installClipboard } from "../../test/clipboard";
import { DataGrid, type GridColumn } from "./DataGrid";

installClipboard();

const columns: GridColumn[] = [
  { name: "id", typeName: "INTEGER", category: "integer", primaryKey: true },
  { name: "name", typeName: "TEXT", category: "text" },
  { name: "owner", typeName: "INTEGER", category: "integer", references: "main.people" },
];
const rows: Value[][] = [
  [
    { t: "int", v: "1" },
    { t: "text", v: "<img src=x onerror=alert(1)>" },
    { t: "int", v: "7" },
  ],
  [{ t: "int", v: "2" }, { t: "null" }, { t: "null" }],
  [
    { t: "int", v: "3" },
    { t: "text", v: "" },
    { t: "int", v: "8" },
  ],
];

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
  clipboardWrite.mockClear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

const cell = (row: number, col: number) =>
  screen.getAllByRole("row")[row + 1]?.querySelector(`[aria-colindex="${col + 2}"]`) as HTMLElement;

describe("DataGrid", () => {
  it("renders headers with key markers and cells as text only", () => {
    render(<DataGrid columns={columns} rows={rows} rowOffset={200} />);

    const header = screen.getByRole("columnheader", { name: "id" });
    expect(header).toHaveTextContent("PK");
    expect(screen.getByRole("columnheader", { name: "owner" })).toHaveTextContent("FK");
    expect(screen.getByText("<img src=x onerror=alert(1)>")).toBeInTheDocument();
    expect(document.querySelector("img")).toBeNull();
    expect(screen.getByText("201")).toBeInTheDocument();
    expect(within(cell(1, 1)).getByText("NULL")).toHaveClass("italic");
  });

  it("selects with clicks and keys, copies the range and activates a cell", async () => {
    const onActivate = vi.fn();
    const onFocusCell = vi.fn();
    render(
      <DataGrid columns={columns} rows={rows} onActivate={onActivate} onFocusCell={onFocusCell} />,
    );
    const grid = screen.getByRole("grid");

    fireEvent.pointerDown(cell(0, 0), { button: 0 });
    fireEvent.pointerDown(cell(1, 1), { button: 0, shiftKey: true });
    expect(cell(0, 1)).toHaveAttribute("aria-selected", "true");
    expect(cell(2, 0)).toHaveAttribute("aria-selected", "false");

    grid.focus();
    fireEvent.keyDown(grid, { key: "c", ctrlKey: true });
    expect(clipboardWrite).toHaveBeenCalledWith("1\t<img src=x onerror=alert(1)>\n2\t");

    fireEvent.keyDown(grid, { key: "ArrowDown" });
    expect(onFocusCell).toHaveBeenLastCalledWith({ row: 2, col: 1 });
    fireEvent.keyDown(grid, { key: "Enter" });
    expect(onActivate).toHaveBeenCalledWith({ row: 2, col: 1 });
    fireEvent.keyDown(grid, { key: "a", ctrlKey: true });
    expect(cell(2, 2)).toHaveAttribute("aria-selected", "true");
    fireEvent.keyDown(grid, { key: "Escape" });
    expect(cell(2, 2)).toHaveAttribute("aria-selected", "false");

    fireEvent.pointerDown(screen.getAllByRole("rowheader")[1] as HTMLElement, {});
    expect(cell(1, 2)).toHaveAttribute("aria-selected", "true");
    await userEvent.dblClick(cell(0, 2));
    expect(onActivate).toHaveBeenLastCalledWith({ row: 0, col: 2 });
  });

  it("drags to select, resizes and auto-sizes columns", () => {
    render(<DataGrid columns={columns} rows={rows} />);

    fireEvent.pointerDown(cell(0, 0), { button: 0 });
    fireEvent.pointerEnter(cell(2, 1));
    fireEvent.pointerUp(cell(2, 1));
    fireEvent.pointerEnter(cell(2, 2));
    expect(cell(2, 1)).toHaveAttribute("aria-selected", "true");
    expect(cell(2, 2)).toHaveAttribute("aria-selected", "false");

    const handle = screen.getByRole("separator", { name: "Resize name" });
    const before = screen.getByRole("columnheader", { name: "name" }).style.width;
    fireEvent.pointerDown(handle, { clientX: 100 });
    fireEvent.pointerMove(window, { clientX: 300 });
    fireEvent.pointerUp(window);
    const after = screen.getByRole("columnheader", { name: "name" }).style.width;
    expect(Number.parseInt(after)).toBeGreaterThan(Number.parseInt(before));
    fireEvent.doubleClick(handle);
    expect(screen.getByRole("columnheader", { name: "name" }).style.width).not.toBe(after);
  });

  it("sorts from the headers and follows foreign keys", async () => {
    const onSort = vi.fn();
    const onFollow = vi.fn();
    render(
      <DataGrid
        columns={columns}
        rows={rows}
        sort={[
          { column: "id", descending: true },
          { column: "name", descending: false },
        ]}
        onSort={onSort}
        onFollow={onFollow}
      />,
    );

    expect(screen.getByRole("columnheader", { name: "id" })).toHaveAttribute(
      "aria-sort",
      "descending",
    );
    expect(screen.getByRole("columnheader", { name: "name" })).toHaveTextContent("▲2");
    fireEvent.click(screen.getByRole("button", { name: "Sort by name" }));
    fireEvent.click(screen.getByRole("button", { name: "Sort by owner" }), { shiftKey: true });
    expect(onSort.mock.calls).toEqual([
      ["name", false],
      ["owner", true],
    ]);

    const links = screen.getAllByRole("button", { name: "Open the main.people row" });
    expect(links).toHaveLength(2);
    await userEvent.click(links[1] as HTMLElement);
    expect(onFollow).toHaveBeenCalledWith({ row: 2, col: 2 });
  });
});
