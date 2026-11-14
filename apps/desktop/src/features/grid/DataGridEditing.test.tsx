import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Value } from "../../ipc";
import type { EditorKind } from "../edit/editors";
import { DataGrid, type GridColumn, type GridEditing, type RowMark } from "./DataGrid";

const columns: GridColumn[] = [
  { name: "id", typeName: "INTEGER", category: "integer" },
  { name: "flag", typeName: "BOOLEAN", category: "bool" },
  { name: "doc", typeName: "JSON", category: "json" },
  { name: "photo", typeName: "BLOB", category: "binary" },
];
const rows: Value[][] = [
  [
    { t: "int", v: "1" },
    { t: "bool", v: true },
    { t: "json", v: "{}" },
    { t: "bytes", v: "", len: 0 },
  ],
  [{ t: "int", v: "2" }, { t: "bool", v: false }, { t: "json", v: "[]" }, { t: "null" }],
];
const KINDS: EditorKind[] = ["number", "bool", "long", "binary"];

function editing(overrides: Partial<GridEditing> = {}): GridEditing {
  return {
    mark: (row): RowMark => ({
      inserted: row === 1,
      deleted: false,
      edited: row === 0 ? ["id"] : [],
      originals: row === 0 ? { id: { t: "int", v: "9" } } : {},
      failed: false,
    }),
    kind: (cell) => KINDS[cell.col] ?? "none",
    initialText: (cell) => (rows[cell.row]?.[cell.col] as { v?: string } | undefined)?.v ?? "",
    parse: (_cell, text) =>
      /^\d+$/.test(text) ? { value: { t: "int", v: text } } : { error: "Enter a whole number" },
    hint: () => undefined,
    onCommit: vi.fn(),
    onToggle: vi.fn(),
    onEditLong: vi.fn(),
    onSetNull: vi.fn(),
    onDeleteRows: vi.fn(),
    ...overrides,
  };
}

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
});

afterEach(() => {
  vi.restoreAllMocks();
});

const cell = (row: number, col: number) =>
  screen.getAllByRole("row")[row + 1]?.querySelector(`[aria-colindex="${col + 2}"]`) as HTMLElement;

describe("DataGrid editing", () => {
  it("marks edited cells with their original value and inserted rows", () => {
    render(<DataGrid columns={columns} rows={rows} edit={editing()} />);

    expect(cell(0, 0)).toHaveAttribute("data-edited", "true");
    expect(cell(0, 0).title).toContain("Original: 9");
    expect(screen.getAllByRole("row")[2]).toHaveAttribute("data-state", "inserted");
  });

  it("edits inline: invalid values stay open, Enter commits and moves down, Escape cancels", async () => {
    const edit = editing();
    render(<DataGrid columns={columns} rows={rows} edit={edit} />);

    await userEvent.dblClick(cell(0, 0));
    const input = screen.getByLabelText("Edit id");
    await userEvent.clear(input);
    await userEvent.type(input, "x{Enter}");
    expect(screen.getByRole("alert")).toHaveTextContent("Enter a whole number");
    await userEvent.clear(input);
    await userEvent.type(input, "42{Enter}");
    expect(edit.onCommit).toHaveBeenCalledWith({ row: 0, col: 0 }, { t: "int", v: "42" });
    expect(screen.queryByLabelText("Edit id")).toBeNull();

    const grid = screen.getByRole("grid");
    fireEvent.keyDown(grid, { key: "7" });
    const typed = screen.getByLabelText("Edit id");
    expect(typed).toHaveValue("7");
    fireEvent.keyDown(typed, { key: "Escape" });
    expect(screen.queryByLabelText("Edit id")).toBeNull();
    expect(edit.onCommit).toHaveBeenCalledTimes(1);
  });

  it("toggles booleans, sends long values to the viewer, and leaves binary alone", async () => {
    const onActivate = vi.fn();
    const edit = editing();
    render(<DataGrid columns={columns} rows={rows} edit={edit} onActivate={onActivate} />);

    await userEvent.dblClick(cell(0, 1));
    expect(edit.onToggle).toHaveBeenCalledWith({ row: 0, col: 1 });
    await userEvent.dblClick(cell(0, 2));
    expect(edit.onEditLong).toHaveBeenCalledWith({ row: 0, col: 2 });
    await userEvent.dblClick(cell(0, 3));
    expect(onActivate).toHaveBeenCalledWith({ row: 0, col: 3 });
  });

  it("sets NULL and deletes rows with the shortcuts, and Tab edits across", () => {
    const edit = editing();
    render(<DataGrid columns={columns} rows={rows} edit={edit} />);
    const grid = screen.getByRole("grid");
    const mod = /Mac/.test(navigator.platform) ? { metaKey: true } : { ctrlKey: true };

    fireEvent.pointerDown(cell(0, 0), { button: 0 });
    fireEvent.pointerDown(cell(1, 1), { button: 0, shiftKey: true });
    fireEvent.keyDown(grid, { key: "Backspace", shiftKey: true, ...mod });
    expect(edit.onSetNull).toHaveBeenCalledWith([
      { row: 0, col: 0 },
      { row: 0, col: 1 },
      { row: 1, col: 0 },
      { row: 1, col: 1 },
    ]);
    fireEvent.keyDown(grid, { key: "Backspace", ...mod });
    expect(edit.onDeleteRows).toHaveBeenCalledWith([0, 1]);

    fireEvent.pointerDown(cell(1, 0), { button: 0 });
    fireEvent.keyDown(grid, { key: "F2" });
    fireEvent.keyDown(screen.getByLabelText("Edit id"), { key: "Tab", shiftKey: true });
    expect(edit.onCommit).toHaveBeenCalledWith({ row: 1, col: 0 }, { t: "int", v: "2" });
  });

  it("does not edit deleted rows", async () => {
    const edit = editing({
      mark: () => ({ inserted: false, deleted: true, edited: [], originals: {}, failed: false }),
    });
    render(<DataGrid columns={columns} rows={rows} edit={edit} />);

    await userEvent.dblClick(cell(0, 0));

    expect(screen.queryByLabelText("Edit id")).toBeNull();
    expect(screen.getAllByRole("row")[1]).toHaveAttribute("data-state", "deleted");
  });
});
