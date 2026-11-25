import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { QueryStats } from "../../ipc";
import { useEditor } from "../editor/editorStore";
import { useWorkspace } from "../workspace/workspaceStore";
import { QueryStatsDialog } from "./QueryStatsDialog";

function answer(stats: QueryStats) {
  mockIPC((cmd) => (cmd === "session_query_stats" ? stats : null));
}

beforeEach(() => {
  useEditor.setState({ tabs: {} });
  useWorkspace.setState({ workspaces: {} });
});

afterEach(() => {
  clearMocks();
});

describe("QueryStatsDialog", () => {
  it("lists statements and opens one in a new editor tab", async () => {
    answer({
      kind: "available",
      statements: [
        {
          statement: "SELECT * FROM t WHERE id = $1",
          calls: 1200,
          totalMs: 2500,
          meanMs: 2.08,
          rows: 1200,
        },
      ],
    });
    const onClose = vi.fn();
    render(<QueryStatsDialog sessionId="s1" onClose={onClose} />);

    expect(await screen.findByText("1,200")).toBeInTheDocument();
    expect(screen.getByText("2.50 s")).toBeInTheDocument();
    expect(screen.getByText("2.08 ms")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "SELECT * FROM t WHERE id = $1" }));

    const tabs = Object.values(useEditor.getState().tabs);
    expect(tabs).toHaveLength(1);
    expect(tabs[0]?.text).toBe("SELECT * FROM t WHERE id = $1");
    expect(onClose).toHaveBeenCalled();
  });

  it("explains how to turn recording on", async () => {
    answer({ kind: "unavailable", reason: "pg_stat_statements is not installed." });
    render(<QueryStatsDialog sessionId="s1" onClose={() => undefined} />);
    expect(await screen.findByText("pg_stat_statements is not installed.")).toBeInTheDocument();
  });
});
