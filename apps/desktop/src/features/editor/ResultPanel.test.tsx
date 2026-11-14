import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { TabState } from "./editorStore";
import { ResultPanel } from "./ResultPanel";

function tab(patch: Partial<TabState>): TabState {
  return {
    tabId: "t",
    text: "",
    revision: 0,
    running: false,
    startedAt: null,
    result: null,
    error: null,
    confirmation: null,
    ...patch,
  };
}

describe("ResultPanel", () => {
  it("prompts before anything has run", () => {
    render(<ResultPanel tab={tab({})} onJumpTo={vi.fn()} />);

    expect(screen.getByRole("status")).toHaveTextContent("Run a statement");
  });

  it("shows progress while running", () => {
    render(<ResultPanel tab={tab({ running: true })} onJumpTo={vi.fn()} />);

    expect(screen.getByRole("status")).toHaveTextContent("Running");
  });

  it("shows affected rows", () => {
    const result = {
      outcome: { kind: "affected" as const, rows: "1" },
      elapsedMs: 2,
      statementClass: "write" as const,
    };

    render(<ResultPanel tab={tab({ result })} onJumpTo={vi.fn()} />);

    expect(screen.getByRole("status")).toHaveTextContent("1 row affected.");
  });

  it("marks truncated results", () => {
    const result = {
      outcome: { kind: "rows" as const, columns: [], rows: [[], []], truncated: true },
      elapsedMs: 2,
      statementClass: "read" as const,
    };

    render(<ResultPanel tab={tab({ result })} onJumpTo={vi.fn()} />);

    expect(screen.getByRole("status")).toHaveTextContent("Showing the first 2 rows");
  });

  it("shows the error with its location and jumps there", async () => {
    const onJumpTo = vi.fn();
    const error = { error: { code: "syntax", message: 'near "FRM": syntax error' }, offset: 19 };

    render(
      <ResultPanel tab={tab({ text: "SELECT 1;\nSELECT * FRM t", error })} onJumpTo={onJumpTo} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "line 2, column 10" }));

    expect(screen.getByRole("alert")).toHaveTextContent('near "FRM": syntax error');
    expect(onJumpTo).toHaveBeenCalledWith(19);
  });

  it("shows errors without a location", () => {
    const error = { error: { code: "read_only", message: "read-only" }, offset: null };

    render(<ResultPanel tab={tab({ error })} onJumpTo={vi.fn()} />);

    expect(screen.getByRole("alert")).toHaveTextContent("read_only");
    expect(screen.queryByRole("button")).toBeNull();
  });
});
