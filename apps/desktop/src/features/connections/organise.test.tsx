import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../../App";
import { createMockBackend } from "../../test/mockBackend";
import { useSessions } from "../sessions/sessionsStore";
import { useConnections } from "./connectionsStore";

beforeEach(() => {
  useConnections.setState({ drivers: [], profiles: [], loaded: false, error: null });
  useSessions.setState({ sessions: {}, activeConnectionId: null, remembered: [] });
  mockIPC(createMockBackend());
});

afterEach(() => {
  clearMocks();
});

async function add(name: string, folder?: string) {
  await userEvent.click(await screen.findByRole("button", { name: "New connection" }));
  const dialog = screen.getByRole("dialog");
  await userEvent.selectOptions(within(dialog).getByLabelText("Database type"), "sqlite");
  await userEvent.type(within(dialog).getByLabelText("Name"), name);
  if (folder) await userEvent.type(within(dialog).getByLabelText("Folder"), folder);
  await userEvent.click(within(dialog).getByRole("button", { name: "Browse…" }));
  await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
  await waitFor(() => {
    expect(screen.queryByRole("dialog")).toBeNull();
  });
}

const names = (list: HTMLElement) =>
  within(list)
    .getAllByRole("button", { name: /^(?!Move|Duplicate|Edit|Delete)/ })
    .map((b) => b.textContent);

describe("organising connections", () => {
  it("groups by folder, duplicates after the original and moves within a folder", async () => {
    render(<App />);
    await add("Alpha", "Prod");
    await add("Beta", "Prod");
    await add("Local");

    const prod = await screen.findByRole("list", { name: "Prod" });
    expect(names(prod)).toEqual(["Alpha", "Beta"]);

    await userEvent.click(within(prod).getByRole("button", { name: "Duplicate Alpha" }));
    await waitFor(() => {
      expect(names(screen.getByRole("list", { name: "Prod" }))).toEqual([
        "Alpha",
        "Alpha copy",
        "Beta",
      ]);
    });

    await userEvent.click(screen.getByRole("button", { name: "Move Beta up" }));
    await waitFor(() => {
      expect(names(screen.getByRole("list", { name: "Prod" }))).toEqual([
        "Alpha",
        "Beta",
        "Alpha copy",
      ]);
    });
    screen.getByRole("button", { name: /^Alpha$/ }).focus();
    await userEvent.keyboard("{Alt>}{ArrowDown}{/Alt}");
    await waitFor(() => {
      expect(names(screen.getByRole("list", { name: "Prod" }))[0]).toBe("Beta");
    });

    await userEvent.click(screen.getByRole("button", { name: "Prod" }));
    expect(screen.queryByRole("list", { name: "Prod" })).toBeNull();
    expect(screen.getByRole("button", { name: /^Local/ })).toBeInTheDocument();
  });
});
