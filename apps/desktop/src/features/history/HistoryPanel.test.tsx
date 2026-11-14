import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { HistoryEntry } from "../../ipc";
import * as ipc from "../../ipc";
import { HistoryPanel } from "./HistoryPanel";

vi.mock("../../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../ipc")>();
  return { ...actual, historyList: vi.fn(), historyClear: vi.fn() };
});

function entry(id: number, statement: string, patch: Partial<HistoryEntry> = {}): HistoryEntry {
  return {
    id,
    connectionId: "c1",
    statement,
    startedAt: 1_700_000_000_000,
    durationMs: 4,
    outcome: "rows",
    rowCount: 3,
    truncated: false,
    error: null,
    ...patch,
  };
}

const page = (start: number, count: number) =>
  Array.from({ length: count }, (_, i) => entry(start - i, `SELECT ${start - i}`));

beforeEach(() => {
  vi.mocked(ipc.historyList).mockReset();
  vi.mocked(ipc.historyClear).mockReset();
});

function renderPanel() {
  const props = { onInsert: vi.fn(), onOpen: vi.fn(), onClose: vi.fn() };
  render(<HistoryPanel connectionId="c1" {...props} />);
  return props;
}

describe("HistoryPanel", () => {
  it("lists entries with their outcome and inserts on click, opens on double-click", async () => {
    vi.mocked(ipc.historyList).mockResolvedValue([
      entry(3, "DELETE FROM t WHERE id = 1", { outcome: "affected", rowCount: 1 }),
      entry(2, "SELECT * FRM t", { outcome: "error", rowCount: null, error: "syntax" }),
      entry(1, "SELECT * FROM big", { truncated: true }),
    ]);
    const props = renderPanel();

    const first = await screen.findByRole("button", { name: /DELETE FROM t/ });
    expect(first).toHaveTextContent("1 affected · 4 ms");
    expect(screen.getByRole("button", { name: /FRM/ })).toHaveTextContent("failed");
    expect(screen.getByRole("button", { name: /big/ })).toHaveTextContent("3+ rows");

    await userEvent.click(first);
    await waitFor(() => {
      expect(props.onInsert).toHaveBeenCalledWith("DELETE FROM t WHERE id = 1");
    });
    await userEvent.dblClick(screen.getByRole("button", { name: /big/ }));
    expect(props.onOpen).toHaveBeenCalledWith("SELECT * FROM big");
    expect(props.onInsert).toHaveBeenCalledTimes(1);
  });

  it("searches, pages with before, and clears", async () => {
    vi.mocked(ipc.historyList)
      .mockResolvedValueOnce(page(60, 50))
      .mockResolvedValueOnce(page(10, 10));
    renderPanel();
    await userEvent.click(await screen.findByRole("button", { name: "Load more" }));
    await screen.findByText("SELECT 1");
    expect(ipc.historyList).toHaveBeenLastCalledWith({
      connectionId: "c1",
      search: undefined,
      limit: 50,
      before: 11,
    });
    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();

    vi.mocked(ipc.historyList).mockResolvedValue([entry(1, "SELECT name FROM users")]);
    await userEvent.type(screen.getByLabelText("Search history"), "users");
    await waitFor(() => {
      expect(ipc.historyList).toHaveBeenLastCalledWith({
        connectionId: "c1",
        search: "users",
        limit: 50,
        before: undefined,
      });
    });
    vi.mocked(ipc.historyClear).mockResolvedValue(undefined);
    await userEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(ipc.historyClear).toHaveBeenCalledWith("c1");
    expect(await screen.findByText("No statements yet.")).toBeInTheDocument();
  });

  it("shows load and clear errors", async () => {
    vi.mocked(ipc.historyList).mockRejectedValue({ code: "internal", message: "store busy" });
    vi.mocked(ipc.historyClear).mockRejectedValue({ code: "internal", message: "cannot clear" });
    const props = renderPanel();

    expect(await screen.findByRole("alert")).toHaveTextContent("store busy");
    await userEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("cannot clear");
    await userEvent.click(screen.getByRole("button", { name: "Close history" }));
    expect(props.onClose).toHaveBeenCalled();
  });
});
