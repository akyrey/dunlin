import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { ColumnMeta, Value } from "../../ipc";
import { clipboardWrite, installClipboard } from "../../test/clipboard";
import { createMockBackend } from "../../test/mockBackend";
import { CopyAs } from "./CopyAs";
import { ExportDialog } from "./ExportDialog";
import { toFormat } from "./format";

installClipboard();

const main = { catalog: null, schema: "main" };
const columns: ColumnMeta[] = [
  { name: "id", typeName: "INTEGER", category: "integer", nullable: null },
  { name: "name", typeName: "TEXT", category: "text", nullable: null },
];
const rows: Value[][] = [
  [
    { t: "int", v: "1" },
    { t: "text", v: "Ada" },
  ],
  [
    { t: "int", v: "2" },
    { t: "text", v: "Grace", truncated: true },
  ],
];
let calls: { cmd: string; args: Record<string, unknown> }[] = [];

beforeEach(() => {
  calls = [];
  const backend = createMockBackend();
  mockIPC((cmd, payload) => {
    calls.push({
      cmd,
      args: (payload as { args?: Record<string, unknown> } | undefined)?.args ?? {},
    });
    return backend(cmd, payload);
  });
  clipboardWrite.mockClear();
});

afterEach(() => {
  clearMocks();
});

const exported = () => calls.find((c) => c.cmd.startsWith("export_"));

describe("ExportDialog", () => {
  it("exports the rows shown as CSV with options, and reports where", async () => {
    const onClose = vi.fn();
    render(
      <ExportDialog
        sessionId="s1"
        name="result"
        columns={columns}
        rows={[rows[0] as Value[]]}
        onClose={onClose}
      />,
    );

    await userEvent.selectOptions(screen.getByLabelText("Delimiter"), "\t");
    await userEvent.click(screen.getByLabelText("Header row"));
    await userEvent.type(screen.getByLabelText("NULL as"), "\\N");
    await userEvent.click(screen.getByRole("button", { name: "Export…" }));

    expect(await screen.findByRole("status")).toHaveTextContent("Wrote 1 rows to /mock/new.db");
    expect(exported()).toMatchObject({
      cmd: "export_result",
      args: {
        format: { kind: "csv", delimiter: "\t", header: false, null: "\\N" },
        path: "/mock/new.db",
      },
    });
    await userEvent.click(screen.getByRole("button", { name: "Show in Finder" }));
    expect(calls.some((c) => c.cmd === "plugin:opener|reveal_item_in_dir")).toBe(true);
    await userEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(onClose).toHaveBeenCalled();
  });

  it("warns about cut values and shows the backend's refusal", async () => {
    render(
      <ExportDialog sessionId="s1" name="result" columns={columns} rows={rows} onClose={vi.fn()} />,
    );

    expect(screen.getByText(/cut short for display/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Export…" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("cut short");
  });

  it("streams the whole table with progress, and can cancel", async () => {
    const table = {
      viewId: "v1",
      table: { namespace: main, name: "events" },
      filter: "pg_sleep(1) > 0",
      key: [],
      sort: [],
    };
    render(
      <ExportDialog
        sessionId="s1"
        name="events"
        columns={columns}
        rows={rows}
        table={table}
        onClose={vi.fn()}
      />,
    );

    await userEvent.selectOptions(screen.getByLabelText("Format"), "insert");
    expect(screen.queryByLabelText("Table")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Export…" }));
    expect(await screen.findByText(/1,000 rows written/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Cancel export" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("canceling statement");
    expect(exported()).toMatchObject({
      cmd: "export_table",
      args: { viewId: "v1:export", format: { kind: "insert", table: table.table, batch: 100 } },
    });
    await waitFor(() => {
      expect(calls.some((c) => c.cmd === "tab_close" && c.args.tabId === "v1:export")).toBe(true);
    });
  });

  it("exports the shown rows of a table as JSON, or INSERTs into a named table", async () => {
    const table = { viewId: "v1", table: { namespace: main, name: "people" }, key: [], sort: [] };
    render(
      <ExportDialog
        sessionId="s1"
        name="people"
        columns={columns}
        rows={[rows[0] as Value[]]}
        table={table}
        onClose={vi.fn()}
      />,
    );

    await userEvent.click(screen.getByLabelText(/rows shown/));
    await userEvent.selectOptions(screen.getByLabelText("Format"), "insert");
    await userEvent.clear(screen.getByLabelText("Table"));
    expect(screen.getByRole("button", { name: "Export…" })).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Table"), "copy");
    await userEvent.click(screen.getByRole("button", { name: "Export…" }));

    await screen.findByText(/Wrote 1 rows/);
    expect(exported()?.args.format).toEqual({
      kind: "insert",
      table: { namespace: { catalog: null, schema: null }, name: "copy" },
      batch: 100,
    });
  });
});

describe("CopyAs", () => {
  it("copies TSV locally and other formats through the backend", async () => {
    render(
      <CopyAs
        sessionId="s1"
        columns={columns}
        rows={rows}
        selection={{ top: 0, bottom: 0, left: 0, right: 1 }}
        insertTable={{ namespace: main, name: "t" }}
      />,
    );

    await userEvent.selectOptions(screen.getByLabelText("Copy as"), "tsv-header");
    await userEvent.selectOptions(screen.getByLabelText("Copy as"), "json");

    await waitFor(() => {
      expect(clipboardWrite.mock.calls).toEqual([["id\tname\n1\tAda"], ['[["1","Ada"]]']]);
    });
    expect(screen.getByRole("status")).toHaveTextContent("Copied 1 row");
  });

  it("copies everything when nothing is selected, and reports failures", async () => {
    clipboardWrite.mockRejectedValueOnce(new Error("clipboard refused"));
    render(
      <CopyAs
        sessionId="s1"
        columns={columns}
        rows={rows}
        selection={null}
        insertTable={{ namespace: main, name: "t" }}
      />,
    );

    await userEvent.selectOptions(screen.getByLabelText("Copy as"), "tsv");

    expect(await screen.findByRole("status")).toHaveTextContent("clipboard refused");
    expect(
      toFormat({ kind: "insert", delimiter: ",", header: true, nullText: "", batch: 0 }, null),
    ).toBeNull();
  });
});
