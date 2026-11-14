import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../../App";
import { createMockBackend } from "../../test/mockBackend";
import { useConnections } from "../connections/connectionsStore";
import { useEditor } from "../editor/editorStore";
import { useSessions } from "../sessions/sessionsStore";

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

async function addAndOpen(name: string) {
  await userEvent.click(await screen.findByRole("button", { name: "New connection" }));
  const dialog = screen.getByRole("dialog");
  await userEvent.selectOptions(within(dialog).getByLabelText("Database type"), "sqlite");
  await userEvent.type(within(dialog).getByLabelText("Name"), name);
  await userEvent.click(within(dialog).getByRole("button", { name: "Browse…" }));
  await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
  await waitFor(() => {
    expect(screen.queryByRole("dialog")).toBeNull();
  });
  await userEvent.click(screen.getByRole("button", { name: new RegExp(`^${name}`) }));
  await screen.findByRole("button", { name: "Run" });
}

describe("command palette", () => {
  it("opens with Ctrl+K and opens a table of the active session", async () => {
    render(<App />);
    await addAndOpen("Local");

    await userEvent.keyboard("{Control>}k{/Control}");
    const palette = await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.type(within(palette).getByRole("combobox"), "peop");
    await within(palette).findByRole("option", { name: /people/, selected: true });
    await userEvent.keyboard("{Enter}");

    expect(screen.queryByRole("dialog", { name: "Command palette" })).toBeNull();
    expect(await screen.findByRole("status", { name: "Rows shown" })).toBeInTheDocument();
  });

  it("moves with the arrows, runs an action and closes with Escape", async () => {
    render(<App />);
    await screen.findByRole("button", { name: "New connection" });

    await userEvent.keyboard("{Control>}k{/Control}");
    await userEvent.type(screen.getByRole("combobox"), "s");
    await userEvent.keyboard("{ArrowDown}{ArrowUp}");
    const first = screen.getAllByRole("option")[0];
    expect(first).toHaveAttribute("aria-selected", "true");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Command palette" })).toBeNull();

    await userEvent.keyboard("{Control>}k{/Control}");
    await userEvent.type(screen.getByRole("combobox"), "keyboard");
    await userEvent.keyboard("{Enter}");
    const reference = await screen.findByRole("dialog", { name: "Keyboard shortcuts" });
    expect(within(reference).getByText("Command palette")).toBeInTheDocument();
    expect(within(reference).getByText("Ctrl+K")).toBeInTheDocument();
  });

  it("opens the settings with Ctrl+,", async () => {
    render(<App />);
    await screen.findByRole("button", { name: "New connection" });

    await userEvent.keyboard("{Control>},{/Control}");

    expect(await screen.findByRole("dialog", { name: "Settings" })).toBeInTheDocument();
  });

  it("says when nothing matches", async () => {
    render(<App />);
    await screen.findByRole("button", { name: "New connection" });

    await userEvent.keyboard("{Control>}k{/Control}");
    await userEvent.type(screen.getByRole("combobox"), "zzzzqq");

    expect(screen.getByText("No matches.")).toBeInTheDocument();
  });
});
