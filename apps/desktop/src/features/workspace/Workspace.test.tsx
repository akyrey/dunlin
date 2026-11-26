import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../../App";
import { createMockBackend } from "../../test/mockBackend";
import { useConnections } from "../connections/connectionsStore";
import { useEditor } from "../editor/editorStore";
import { saveTabs } from "../editor/persistence";
import { useSchema } from "../schema/schemaStore";
import { useSessions } from "../sessions/sessionsStore";
import { useWorkspace } from "./workspaceStore";

const mod = /Mac/.test(navigator.platform) ? { metaKey: true } : { ctrlKey: true };

beforeEach(() => {
  useConnections.setState({ drivers: [], profiles: [], loaded: false, error: null });
  useSessions.setState({ sessions: {}, activeConnectionId: null, notice: null, unreachable: {} });
  useEditor.setState({ tabs: {} });
  useSchema.setState({ trees: {}, filter: "", showSystem: false });
  useWorkspace.setState({ workspaces: {} });
  mockIPC(createMockBackend());
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
});

afterEach(() => {
  clearMocks();
  vi.restoreAllMocks();
});

async function open() {
  await userEvent.click(await screen.findByRole("button", { name: "New connection" }));
  const dialog = screen.getByRole("dialog");
  await userEvent.selectOptions(within(dialog).getByLabelText("Database type"), "sqlite");
  await userEvent.type(within(dialog).getByLabelText("Name"), "People");
  await userEvent.click(within(dialog).getByRole("button", { name: "Browse…" }));
  await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
  await waitFor(() => {
    expect(screen.queryByRole("dialog")).toBeNull();
  });
  await userEvent.click(screen.getByRole("button", { name: /^People/ }));
  return screen.findByRole("tablist", { name: "Open tabs" });
}

const titles = (strip: HTMLElement) =>
  within(strip)
    .getAllByRole("tab")
    .map((t) => t.textContent);

describe("workspace tabs", () => {
  it("opens, renames, switches and closes tabs", async () => {
    render(<App />);
    const strip = await open();
    expect(titles(strip)).toEqual(["Query 1"]);

    await userEvent.click(screen.getByRole("button", { name: "New tab" }));
    fireEvent.keyDown(window, { key: "t", ...mod });
    expect(titles(strip)).toEqual(["Query 1", "Query 2", "Query 3"]);

    await userEvent.dblClick(within(strip).getByRole("tab", { name: "Query 3" }));
    const name = screen.getByLabelText("Tab name");
    await userEvent.clear(name);
    await userEvent.type(name, "Reports{Enter}");
    expect(titles(strip)).toEqual(["Query 1", "Query 2", "Reports"]);

    fireEvent.keyDown(window, { key: "Tab", ctrlKey: true });
    expect(within(strip).getByRole("tab", { selected: true })).toHaveTextContent("Query 1");
    fireEvent.keyDown(window, { key: "Tab", ctrlKey: true, shiftKey: true });
    expect(within(strip).getByRole("tab", { selected: true })).toHaveTextContent("Reports");

    fireEvent.keyDown(window, { key: "w", ...mod });
    await userEvent.click(screen.getByRole("button", { name: "Close Query 1" }));
    expect(titles(strip)).toEqual(["Query 2"]);
  });

  it("says when the server stops answering, and stops saying so when it is back", async () => {
    render(<App />);
    await open();
    const sessionId = Object.values(useSessions.getState().sessions)[0]?.sessionId ?? "";

    act(() => {
      useSessions.getState().setHealth({ sessionId, status: "unreachable" });
    });
    expect(screen.getByRole("alert")).toHaveTextContent("The server is not answering");
    act(() => {
      useSessions.getState().setHealth({ sessionId, status: "ok" });
    });
    expect(screen.queryByText(/The server is not answering/)).toBeNull();
  });

  it("asks before closing a tab with a running statement", async () => {
    render(<App />);
    await open();
    const tabId = Object.keys(useEditor.getState().tabs)[0] ?? "";
    useEditor.getState().setText(tabId, "SELECT pg_sleep(30)");
    await userEvent.click(screen.getByRole("button", { name: "Run" }));
    await screen.findByRole("button", { name: "Cancel" });

    await userEvent.click(screen.getByRole("button", { name: "Close Query 1" }));
    const dialog = screen.getByRole("dialog", { name: "Close a running tab?" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Close tab" }));

    await waitFor(() => {
      expect(useEditor.getState().tabs).toEqual({});
    });
    expect(screen.getByText(/Open a new tab/)).toBeInTheDocument();
  });

  it("restores the connection's saved tabs when it opens", async () => {
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "New connection" }));
    const dialog = screen.getByRole("dialog");
    await userEvent.selectOptions(within(dialog).getByLabelText("Database type"), "sqlite");
    await userEvent.type(within(dialog).getByLabelText("Name"), "People");
    await userEvent.click(within(dialog).getByRole("button", { name: "Browse…" }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    const id = (await vi.waitFor(() => useConnections.getState().profiles[0]))?.id ?? "";
    saveTabs(id, [
      { title: "Saved", text: "SELECT 1", rowLimit: 100 },
      { title: "Other", text: "SELECT 2", rowLimit: 1000 },
    ]);

    await userEvent.click(screen.getByRole("button", { name: /^People/ }));

    const strip = await screen.findByRole("tablist", { name: "Open tabs" });
    expect(titles(strip)).toEqual(["Saved", "Other"]);
    expect(screen.getByLabelText("SQL editor")).toHaveTextContent("SELECT 1");
    expect(screen.getByLabelText("Row limit")).toHaveValue("100");
  });

  it("formats the editor's SQL, and undo brings the original back", async () => {
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "New connection" }));
    const dialog = screen.getByRole("dialog");
    await userEvent.selectOptions(within(dialog).getByLabelText("Database type"), "sqlite");
    await userEvent.type(within(dialog).getByLabelText("Name"), "People");
    await userEvent.click(within(dialog).getByRole("button", { name: "Browse…" }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    const id = (await vi.waitFor(() => useConnections.getState().profiles[0]))?.id ?? "";
    saveTabs(id, [{ title: "Messy", text: "select a, b from people", rowLimit: 100 }]);
    await userEvent.click(screen.getByRole("button", { name: /^People/ }));
    await screen.findByRole("tablist", { name: "Open tabs" });

    await userEvent.click(screen.getByRole("button", { name: "Format" }));

    const tabText = () => Object.values(useEditor.getState().tabs)[0]?.text;
    expect(tabText()).toBe("select\n  a,\n  b\nfrom\n  people");
    fireEvent.keyDown(screen.getByLabelText("SQL editor"), { key: "z", code: "KeyZ", ...mod });
    expect(tabText()).toBe("select a, b from people");
  });

  it("asks before closing a data tab with unapplied edits", async () => {
    render(<App />);
    const strip = await open();
    await userEvent.click(screen.getByRole("button", { name: "people" }));
    await userEvent.dblClick(await screen.findByRole("gridcell", { name: "Grace" }));
    await userEvent.type(screen.getByLabelText("Edit name"), "!{Enter}");

    await userEvent.click(within(strip).getByRole("button", { name: "Close people" }));
    const dialog = screen.getByRole("dialog", { name: "Discard unapplied changes?" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(titles(strip)).toContain("▦people");
    await userEvent.click(within(strip).getByRole("button", { name: "Close people" }));
    await userEvent.click(screen.getByRole("button", { name: "Discard and close" }));

    expect(titles(strip)).not.toContain("▦people");
  });

  it("focuses the results with the shortcut", async () => {
    render(<App />);
    await open();

    fireEvent.keyDown(window, { key: "®", code: "KeyR", altKey: true, ...mod });

    expect(screen.getByRole("region", { name: "Results" })).toHaveFocus();
  });
});
