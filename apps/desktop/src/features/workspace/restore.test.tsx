import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../../App";
import { createMockBackend } from "../../test/mockBackend";
import { useConnections } from "../connections/connectionsStore";
import { useEditor } from "../editor/editorStore";
import { useSessions } from "../sessions/sessionsStore";
import { loadLayout } from "./layout";

const restart = () => {
  useConnections.setState({ drivers: [], profiles: [], loaded: false, error: null });
  useSessions.setState({ sessions: {}, activeConnectionId: null, remembered: [] });
  useEditor.setState({ tabs: {} });
};

beforeEach(() => {
  restart();
  mockIPC(createMockBackend());
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
});

afterEach(() => {
  clearMocks();
  vi.restoreAllMocks();
});

async function add(name: string) {
  await userEvent.click(await screen.findByRole("button", { name: "New connection" }));
  const dialog = screen.getByRole("dialog");
  await userEvent.selectOptions(within(dialog).getByLabelText("Database type"), "sqlite");
  await userEvent.type(within(dialog).getByLabelText("Name"), name);
  await userEvent.click(within(dialog).getByRole("button", { name: "Browse…" }));
  await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
  await waitFor(() => {
    expect(screen.queryByRole("dialog")).toBeNull();
  });
}

const connection = (name: string) =>
  within(screen.getByRole("list", { name: "Connections" })).getByRole("button", {
    name: new RegExp(`^${name}`),
  });

describe("restoring the previous layout", () => {
  it("reopens the active connection, remembers the others and keeps the sidebar width", async () => {
    const first = render(<App />);
    await add("First");
    await add("Second");
    await userEvent.click(connection("Second"));
    await screen.findByRole("button", { name: "Run" });
    await userEvent.click(connection("First"));
    await waitFor(() => {
      expect(useSessions.getState().activeConnectionId).not.toBeNull();
    });
    const separator = screen.getByRole("separator", { name: "Resize the sidebar" });
    fireEvent.keyDown(separator, { key: "ArrowRight" });
    await waitFor(() => {
      expect(loadLayout().openConnections).toHaveLength(2);
    });
    const firstId = useSessions.getState().activeConnectionId;
    first.unmount();
    restart();

    render(<App />);

    await waitFor(() => {
      expect(useSessions.getState().activeConnectionId).toBe(firstId);
    });
    expect(await screen.findByRole("button", { name: "Run" })).toBeInTheDocument();
    expect(connection("Second")).toHaveTextContent("(open last time)");
    expect(screen.getByRole("separator", { name: "Resize the sidebar" })).toHaveAttribute(
      "aria-valuenow",
      "272",
    );

    await userEvent.click(connection("Second"));
    await waitFor(() => {
      expect(connection("Second")).not.toHaveTextContent("(open last time)");
    });
  });
});
