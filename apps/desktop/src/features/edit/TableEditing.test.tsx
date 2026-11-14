import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { createMockBackend } from "../../test/mockBackend";
import { TableData } from "../data/TableData";
import { DEFAULT_SETTINGS } from "../settings/defaults";
import { useSettings } from "../settings/settingsStore";
import { pendingViews, useEdits } from "./editStore";
import { readOnlyReason } from "./useTableEditing";

const main = { catalog: null, schema: "main" };
let calls: { cmd: string; args: Record<string, unknown> }[] = [];

beforeEach(() => {
  calls = [];
  useEdits.setState({ pending: {}, previewSkipped: {} });
  useSettings.setState({ settings: DEFAULT_SETTINGS });
  const backend = createMockBackend();
  mockIPC((cmd, payload) => {
    calls.push({
      cmd,
      args: (payload as { args?: Record<string, unknown> } | undefined)?.args ?? {},
    });
    return backend(cmd, payload);
  });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
});

afterEach(() => {
  clearMocks();
  vi.restoreAllMocks();
});

function show(name: string, alwaysPreview = false) {
  return render(
    <TableData
      sessionId="s1"
      connectionId="c1"
      table={{ namespace: main, name }}
      match={[]}
      onFollow={vi.fn()}
      viewKey={`view:${name}`}
      readOnly={false}
      profile={{ name: "People", color: "#22aa88", alwaysPreview }}
    />,
  );
}

const cell = (text: string) => screen.getByRole("gridcell", { name: text });
const pendingStatus = () => screen.getByRole("status", { name: "Pending changes" });

async function rename(from: string, to: string) {
  await userEvent.dblClick(await screen.findByRole("gridcell", { name: from }));
  const editor = screen.getByLabelText("Edit name");
  await userEvent.clear(editor);
  await userEvent.type(editor, `${to}{Enter}`);
}

async function applyThroughPreview(skip = false) {
  await userEvent.click(screen.getByRole("button", { name: "Apply changes" }));
  const dialog = await screen.findByRole("dialog", { name: "Apply changes?" });
  if (skip) await userEvent.click(within(dialog).getByRole("checkbox"));
  await userEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
}

describe("table editing", () => {
  it("reports pending edits for the guards, and nothing reaches the backend before Apply", async () => {
    show("people");

    await rename("Grace", "Grace H.");

    expect(pendingStatus()).toHaveTextContent("1 edited");
    expect(pendingViews(useEdits.getState(), "s1")).toEqual(["view:people"]);
    expect(calls.some((c) => c.cmd === "changes_apply")).toBe(false);
    await userEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(screen.queryByRole("status", { name: "Pending changes" })).toBeNull();
    expect(cell("Grace")).toBeInTheDocument();
    expect(pendingViews(useEdits.getState())).toEqual([]);
  });

  it("can skip the preview for the rest of the session after seeing it once", async () => {
    show("people");
    await rename("Grace", "One");
    await applyThroughPreview(true);
    await screen.findByText(/Applied 1 statement/);

    await rename("One", "Two");
    await userEvent.click(screen.getByRole("button", { name: "Apply changes" }));

    await screen.findByRole("gridcell", { name: "Two" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls.filter((c) => c.cmd === "changes_preview")).toHaveLength(1);
  });

  it("always previews on a profile that asks for it", async () => {
    useEdits.getState().skipPreview("s1");
    show("people", true);
    await rename("Grace", "G");

    await userEvent.click(screen.getByRole("button", { name: "Apply changes" }));

    const dialog = await screen.findByRole("dialog", { name: "Apply changes?" });
    expect(within(dialog).queryByRole("checkbox")).toBeNull();
    expect(within(dialog).getByLabelText("Statements to apply")).toHaveTextContent(
      "UPDATE people SET name = 'G'",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(pendingStatus()).toHaveTextContent("1 edited");
  });

  it("always previews when the settings ask for it, whatever the profile says", async () => {
    useEdits.getState().skipPreview("s1");
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, previewPolicy: "always" } });
    show("people");
    await rename("Grace", "G");

    await userEvent.click(screen.getByRole("button", { name: "Apply changes" }));

    const dialog = await screen.findByRole("dialog", { name: "Apply changes?" });
    expect(within(dialog).queryByRole("checkbox")).toBeNull();
  });

  it("duplicates a row without its key, sets NULL and reverts from the toolbar", async () => {
    show("people");
    await screen.findByRole("gridcell", { name: "Ada" });

    fireEvent.pointerDown(cell("Ada"), { button: 0 });
    await userEvent.click(screen.getByRole("button", { name: "Duplicate" }));
    expect(pendingStatus()).toHaveTextContent("1 new");
    const inserted = document.querySelector('[data-state="inserted"]');
    expect(inserted).toHaveTextContent("DEFAULT");
    expect(inserted).toHaveTextContent("Ada");

    fireEvent.pointerDown(cell("Grace"), { button: 0 });
    await userEvent.click(screen.getByRole("button", { name: "Set NULL" }));
    expect(pendingStatus()).toHaveTextContent("1 edited · 1 new");
    fireEvent.pointerDown(document.querySelector('[data-edited="true"]') as Element, { button: 0 });
    await userEvent.click(screen.getByRole("button", { name: "Revert" }));
    expect(pendingStatus()).toHaveTextContent("1 new");
    fireEvent.pointerDown(inserted?.querySelector('[aria-colindex="3"]') as Element, { button: 0 });
    await userEvent.click(screen.getByRole("button", { name: "Revert" }));
    expect(screen.queryByRole("status", { name: "Pending changes" })).toBeNull();
  });

  it("asks before refreshing with pending edits, and can apply first", async () => {
    show("people");
    await rename("Grace", "Applied first");

    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    const dialog = screen.getByRole("dialog", { name: "Unapplied changes" });
    expect(dialog).toHaveTextContent("to refresh");
    await userEvent.click(within(dialog).getByRole("button", { name: "Apply" }));

    await waitFor(() => {
      expect(calls.some((c) => c.cmd === "changes_apply")).toBe(true);
    });
    expect(await screen.findByRole("gridcell", { name: "Applied first" })).toBeInTheDocument();
    expect(screen.queryByRole("status", { name: "Pending changes" })).toBeNull();
  });

  it("edits JSON in the cell viewer with validation", async () => {
    show("documents");
    await userEvent.dblClick(await screen.findByRole("gridcell", { name: /broken/ }));

    const editor = await screen.findByLabelText("Edit body");
    await userEvent.clear(editor);
    await userEvent.type(editor, "{{oops");
    await userEvent.click(screen.getByRole("button", { name: "Set value" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Not valid JSON");
    await userEvent.clear(editor);
    await userEvent.type(editor, '{{"fixed": true}');
    await userEvent.click(screen.getByRole("button", { name: "Set value" }));

    expect(pendingStatus()).toHaveTextContent("1 edited");
  });

  it("explains why a table cannot be edited", () => {
    const info = { kind: "table", rowKey: ["id"] } as Parameters<typeof readOnlyReason>[0];
    expect(readOnlyReason(info, true)).toMatch(/Read-only connection/);
    expect(readOnlyReason(null, false)).toMatch(/Loading/);
    expect(readOnlyReason({ ...info, kind: "view" } as typeof info, false)).toBe(
      "Views cannot be edited.",
    );
    expect(readOnlyReason({ ...info, rowKey: null } as typeof info, false)).toMatch(
      /no primary key/,
    );
    expect(readOnlyReason(info, false)).toBeNull();
  });
});
