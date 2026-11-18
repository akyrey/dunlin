import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Value } from "../../ipc";
import { copyText } from "../../lib/clipboard";
import { replaceEditorText } from "../../test/codeEditor";
import { CellViewer, type ViewerEdit } from "./CellViewer";
import type { GridColumn } from "./DataGrid";

vi.mock("../../lib/clipboard", () => ({ copyText: vi.fn(() => Promise.resolve()) }));

const json: GridColumn = { name: "body", typeName: "JSON", category: "json" };
const text: GridColumn = { name: "note", typeName: "TEXT", category: "text" };
const bytes: GridColumn = { name: "photo", typeName: "BLOB", category: "binary" };

function show(column: GridColumn, value: Value, loadFull: (() => Promise<Value>) | null = null) {
  const onClose = vi.fn();
  render(
    <CellViewer
      column={column}
      value={value}
      loadFull={loadFull}
      noFullReason="No row key."
      onClose={onClose}
    />,
  );
  return onClose;
}

describe("CellViewer", () => {
  it("pretty-prints JSON and copies the raw value", async () => {
    show(json, { t: "json", v: '{"a":[1,2]}' });

    expect(screen.getByLabelText("Cell value")).toHaveTextContent('"a": [');
    await userEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect(copyText).toHaveBeenCalledWith('{"a":[1,2]}');
  });

  it("shows text that is not valid JSON as text, without throwing", () => {
    show(json, { t: "text", v: "{oops" });

    expect(screen.getByText(/Not valid JSON/)).toBeInTheDocument();
    expect(screen.getByLabelText("Cell value")).toHaveTextContent("{oops");
  });

  it("pretty-prints XML and keeps markup as text", () => {
    show({ name: "doc", typeName: "xml", category: "xml" }, { t: "xml", v: "<a><b>x</b></a>" });

    expect(screen.getByLabelText("Cell value")).toHaveTextContent("<a>");
    expect(document.querySelector("b")).toBeNull();
  });

  it("loads a truncated value in full, and reports a failure", async () => {
    const loadFull = vi
      .fn<() => Promise<Value>>()
      .mockRejectedValueOnce({ code: "not_found", message: "The row no longer exists" })
      .mockResolvedValueOnce({ t: "text", v: "the whole story" });
    show(text, { t: "text", v: "the whole", truncated: true }, loadFull);

    await userEvent.click(screen.getByRole("button", { name: "Load the whole value" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("no longer exists");
    await userEvent.click(screen.getByRole("button", { name: "Load the whole value" }));
    expect(await screen.findByLabelText("Cell value")).toHaveTextContent("the whole story");
    expect(screen.queryByText(/Showing the start/)).toBeNull();
  });

  it("says why a truncated value cannot be loaded", () => {
    show(text, { t: "text", v: "cut", truncated: true });

    expect(screen.getByRole("status")).toHaveTextContent("No row key.");
    expect(screen.queryByRole("button", { name: "Load the whole value" })).toBeNull();
  });

  it("shows binary as a hex dump with an image preview, and NULL plainly", async () => {
    URL.createObjectURL = vi.fn(() => "blob:preview");
    URL.revokeObjectURL = vi.fn();
    const png = btoa(String.fromCharCode(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a));
    const onClose = show(bytes, { t: "bytes", v: png, len: 2048, truncated: true });

    expect(screen.getByLabelText("Hex dump")).toHaveTextContent("89 50 4e 47");
    expect(screen.getByText(/first 8 B shown/)).toBeInTheDocument();
    expect(screen.getByRole("img")).toHaveAttribute("src", "blob:preview");
    await userEvent.click(screen.getByRole("button", { name: "Close cell viewer" }));
    expect(onClose).toHaveBeenCalled();
  });

  it.each([
    ['{"a":1}', '"a": 1', "holds JSON"],
    ["[1, 2]", "1,", "holds JSON"],
    ["<root><item>x</item></root>", "<item>x</item>", "holds XML"],
  ])("detects %s in a text column", (raw, shown, note) => {
    show(text, { t: "text", v: raw });

    expect(screen.getByLabelText("Cell value")).toHaveTextContent(shown);
    expect(screen.getByText(new RegExp(note))).toBeInTheDocument();
  });

  it.each(["{not json", "<not xml", "plain words", "  "])(
    "keeps %j in a text column as text",
    (raw) => {
      show(text, { t: "text", v: raw });

      expect(screen.queryByText(/holds/)).toBeNull();
    },
  );

  it("does not try to parse a text value that was cut short", () => {
    show(text, { t: "text", v: '{"a": 1}', truncated: true });

    expect(screen.queryByText(/holds JSON/)).toBeNull();
  });

  it("shows NULL as NULL", () => {
    show(text, { t: "null" });

    expect(screen.getByText("The value is NULL.")).toBeInTheDocument();
  });

  describe("editing", () => {
    function edit(column: GridColumn, raw: string, onSave = vi.fn()) {
      const viewerEdit: ViewerEdit = {
        initialText: raw,
        parse: (draft) =>
          draft.startsWith("{") ? { value: { t: "text", v: draft } } : { error: "Bad" },
        onSave,
        start: true,
      };
      render(
        <CellViewer
          column={column}
          value={{ t: "text", v: raw }}
          loadFull={null}
          noFullReason=""
          onClose={vi.fn()}
          edit={viewerEdit}
        />,
      );
      return onSave;
    }

    /** Highlighted tokens are spans with a generated class inside a line. */
    const highlighted = (editor: HTMLElement) =>
      editor.querySelectorAll(".cm-line span:not([class^='cm-'])").length;

    it("highlights text that holds JSON when the viewer opens to edit", () => {
      edit(text, '{"a": 1, "b": true}');

      const editor = screen.getByLabelText("Edit note");
      expect(editor).toHaveTextContent('{"a": 1, "b": true}');
      expect(highlighted(editor)).toBeGreaterThan(0);
    });

    it("does not highlight plain text", () => {
      edit(text, "plain words 1 true");

      expect(highlighted(screen.getByLabelText("Edit note"))).toBe(0);
    });

    it("saves the edited text, and reports text that does not parse", async () => {
      const onSave = edit(text, '{"a": 1}');
      const editor = screen.getByLabelText("Edit note");

      replaceEditorText(editor, "oops");
      await userEvent.click(screen.getByRole("button", { name: "Set value" }));
      expect(screen.getByRole("alert")).toHaveTextContent("Bad");

      replaceEditorText(editor, '{"b": 2}');
      expect(screen.queryByRole("alert")).toBeNull();
      await userEvent.click(screen.getByRole("button", { name: "Set value" }));
      expect(onSave).toHaveBeenCalledWith({ t: "text", v: '{"b": 2}' });
    });
  });
});
