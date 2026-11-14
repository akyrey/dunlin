import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "./App";
import { useConnections } from "./features/connections/connectionsStore";
import { useEditor } from "./features/editor/editorStore";
import { useSessions } from "./features/sessions/sessionsStore";
import { MOCK_DATABASE_PATH, createMockBackend } from "./test/mockBackend";

beforeEach(() => {
  useConnections.setState({ drivers: [], profiles: [], loaded: false, error: null });
  useSessions.setState({ sessions: {}, activeConnectionId: null });
  useEditor.setState({ tabs: {} });
  mockIPC(createMockBackend());
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
});

afterEach(() => {
  clearMocks();
  vi.restoreAllMocks();
});

async function addConnection(name: string, readOnly = false) {
  await userEvent.click(await screen.findByRole("button", { name: "New connection" }));
  const dialog = screen.getByRole("dialog");
  await userEvent.type(within(dialog).getByLabelText("Name"), name);
  await userEvent.click(within(dialog).getByRole("button", { name: "Browse…" }));
  if (readOnly) await userEvent.click(within(dialog).getByLabelText("Read-only"));
  await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
  await waitFor(() => {
    expect(screen.queryByRole("dialog")).toBeNull();
  });
}

async function openAndRun(name: string, sql: string) {
  await userEvent.click(screen.getByRole("button", { name: new RegExp(`^${name}`) }));
  await screen.findByRole("button", { name: "Run" });
  const sessionId = Object.keys(useEditor.getState().tabs)[0] ?? "";
  useEditor.getState().setText(sessionId, sql);
  await userEvent.click(screen.getByRole("button", { name: "Run" }));
}

describe("App", () => {
  it("adds a connection, opens it, runs a query and shows rows", async () => {
    render(<App />);

    await addConnection("People");
    expect(screen.getByRole("list", { name: "Connections" })).toHaveTextContent("People");

    await openAndRun("People", "SELECT * FROM people");

    expect(await screen.findByText("Ada")).toBeInTheDocument();
    expect(screen.getByText("9007199254740993")).toBeInTheDocument();
    expect(screen.getByText("NULL")).toBeInTheDocument();
    expect(screen.getByText("3 rows · 3 ms")).toBeInTheDocument();
    expect(useConnections.getState().profiles[0]?.filePath).toBe(MOCK_DATABASE_PATH);
  });

  it("shows the server error for a syntax error", async () => {
    render(<App />);
    await addConnection("People");

    await openAndRun("People", "SELECT * FRM people");

    expect(await screen.findByRole("alert")).toHaveTextContent('near "FRM": syntax error');
    expect(screen.getByRole("button", { name: "line 1, column 10" })).toBeInTheDocument();
  });

  it("asks before a dangerous statement and runs it on confirmation", async () => {
    render(<App />);
    await addConnection("People");

    await openAndRun("People", "DELETE FROM people");
    const dialog = await screen.findByRole("dialog", { name: "Run a dangerous statement?" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Run anyway" }));

    expect(await screen.findByText("2 rows affected.")).toBeInTheDocument();
  });

  it("rejects writes on a read-only connection", async () => {
    render(<App />);
    await addConnection("Prod", true);

    await openAndRun("Prod", "INSERT INTO people VALUES (4)");

    expect(await screen.findByRole("alert")).toHaveTextContent("read-only");
    expect(screen.getByText("read-only", { selector: "span" })).toBeInTheDocument();
  });

  it("deletes a connection after confirmation", async () => {
    render(<App />);
    await addConnection("People");

    await userEvent.click(screen.getByRole("button", { name: "Delete People" }));
    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Delete" }),
    );

    await waitFor(() => {
      expect(screen.getByText("No connections yet.")).toBeInTheDocument();
    });
  });

  it("edits a connection", async () => {
    render(<App />);
    await addConnection("People");

    await userEvent.click(screen.getByRole("button", { name: "Edit People" }));
    const name = within(screen.getByRole("dialog")).getByLabelText("Name");
    await userEvent.clear(name);
    await userEvent.type(name, "Renamed");
    await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Save" }));

    expect(await screen.findByRole("button", { name: /^Renamed/ })).toBeInTheDocument();
  });

  it("shows an error when a session cannot be opened", async () => {
    const backend = createMockBackend();
    mockIPC((cmd, payload) => {
      if (cmd === "session_open")
        return Promise.reject({ code: "connect", message: "unable to open" });
      return backend(cmd, payload);
    });
    useConnections.setState({ profiles: [], drivers: [] });
    render(<App />);
    await addConnection("Broken");

    await userEvent.click(screen.getByRole("button", { name: /^Broken/ }));

    expect(await screen.findByRole("alert")).toHaveTextContent("unable to open");
  });
});
