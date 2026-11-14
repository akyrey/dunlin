import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../../App";
import { createMockBackend } from "../../test/mockBackend";
import { useConnections } from "../connections/connectionsStore";
import { useEditor } from "../editor/editorStore";
import { useSessions } from "../sessions/sessionsStore";
import { useWorkspace } from "../workspace/workspaceStore";
import { queriesFor, useSavedQueries } from "./savedQueriesStore";

beforeEach(() => {
  useConnections.setState({ drivers: [], profiles: [], loaded: false, error: null });
  useSessions.setState({ sessions: {}, activeConnectionId: null, notice: null });
  useEditor.setState({ tabs: {} });
  useWorkspace.setState({ workspaces: {} });
  useSavedQueries.setState({ queries: [], error: null });
  mockIPC(createMockBackend());
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
});

afterEach(() => {
  clearMocks();
  vi.restoreAllMocks();
});

async function openPeople() {
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
  await screen.findByRole("tablist", { name: "Open tabs" });
}

const list = () => screen.getByRole("region", { name: "Saved queries" });

describe("saved queries", () => {
  it("saves a tab, opens it again and saves back to it", async () => {
    render(<App />);
    await openPeople();
    const tabId = Object.keys(useEditor.getState().tabs)[0] ?? "";
    useEditor.getState().setText(tabId, "SELECT * FROM people");

    await userEvent.click(screen.getByRole("button", { name: "Save as…" }));
    const dialog = screen.getByRole("dialog", { name: "Save query" });
    await userEvent.clear(within(dialog).getByLabelText("Name"));
    await userEvent.type(within(dialog).getByLabelText("Name"), "Everyone");
    await userEvent.click(within(dialog).getByLabelText("Every connection"));
    await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(await within(list()).findByRole("button", { name: /^Everyone/ })).toHaveTextContent(
      "all",
    );
    expect(screen.getByRole("tab", { name: "Everyone" })).toBeInTheDocument();
    useEditor.getState().setText(tabId, "SELECT name FROM people");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(useSavedQueries.getState().queries[0]?.sql).toBe("SELECT name FROM people");
    });

    await userEvent.click(within(list()).getByRole("button", { name: /^Everyone/ }));
    expect(screen.getAllByRole("tab", { name: "Everyone" })).toHaveLength(1);
  });

  it("opens a saved query in a new tab, renames and deletes it", async () => {
    render(<App />);
    await openPeople();
    const connectionId = useConnections.getState().profiles[0]?.id ?? "";
    await useSavedQueries
      .getState()
      .save({ id: null, connectionId, name: "Local only", sql: "SELECT 1" });

    await userEvent.click(await within(list()).findByRole("button", { name: "Local only" }));
    expect(screen.getByRole("tab", { name: "Local only", selected: true })).toBeInTheDocument();

    await userEvent.click(within(list()).getByRole("button", { name: "Rename Local only" }));
    const name = within(list()).getByLabelText("Query name");
    await userEvent.clear(name);
    await userEvent.type(name, "Renamed{Enter}");
    expect(await within(list()).findByRole("button", { name: "Renamed" })).toBeInTheDocument();

    await userEvent.click(within(list()).getByRole("button", { name: "Delete Renamed" }));
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => {
      expect(within(list()).queryByRole("button", { name: "Renamed" })).toBeNull();
    });
    expect(screen.getByText(/Save a tab's SQL/)).toBeInTheDocument();
  });

  it("lists a connection's own queries before those for every connection", () => {
    const query = (id: string, connectionId: string | null) => ({
      id,
      connectionId,
      name: id,
      sql: "",
      updatedAt: 0,
    });
    const all = [query("global", null), query("mine", "c1"), query("other", "c2")];

    expect(queriesFor(all, "c1").map((q) => q.id)).toEqual(["mine", "global"]);
    expect(queriesFor(all, null).map((q) => q.id)).toEqual(["global"]);
  });

  it("asks for a connection before opening a query for every connection", async () => {
    await useSavedQueries
      .getState()
      .save({ id: null, connectionId: null, name: "Anywhere", sql: "SELECT 1" });
    render(<App />);

    await userEvent.click(await within(list()).findByRole("button", { name: /^Anywhere/ }));

    expect(within(list()).getByRole("alert")).toHaveTextContent("Open a connection");
  });
});
