import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { QueryResult } from "../../ipc";
import type { EditorTab } from "./editorStore";
import { ResultPanel } from "./ResultPanel";
import { type StatementRun, toStatements } from "./run";

function tab(patch: Partial<EditorTab>): EditorTab {
  return {
    tabId: "t",
    sessionId: "s1",
    title: "Query 1",
    text: "",
    revision: 0,
    rowLimit: 1000,
    running: false,
    startedAt: null,
    run: null,
    notice: null,
    confirmation: null,
    shown: 0,
    split: null,
    savedQueryId: null,
    ...patch,
  };
}

const affected: QueryResult = {
  outcome: { kind: "affected", rows: "1" },
  elapsedMs: 2,
  statementClass: "write",
};
const truncated: QueryResult = {
  outcome: { kind: "rows", columns: [], rows: [[], []], truncated: true },
  elapsedMs: 2,
  statementClass: "read",
};

function statement(sql: string, start: number, patch: Partial<StatementRun>): StatementRun {
  const [base] = toStatements(sql, [{ start: 0, end: sql.length, danger: null }]);
  if (!base) throw new Error("no statement");
  return { ...base, start, end: start + sql.length, ...patch };
}

function withRun(statements: StatementRun[], patch: Partial<EditorTab> = {}): EditorTab {
  return tab({
    run: {
      statements,
      messages: [{ at: 0, level: "info", text: "Running 3 statements" }],
      cancelRequested: false,
    },
    ...patch,
  });
}

const props = { onShow: vi.fn(), onJumpTo: vi.fn() };

describe("ResultPanel", () => {
  it("prompts before anything has run", () => {
    render(<ResultPanel tab={tab({})} {...props} />);

    expect(screen.getByRole("status")).toHaveTextContent(
      "Run the statement at the cursor with Ctrl+↩",
    );
  });

  it("shows one sub-tab per statement plus Messages, and the shown statement's outcome", () => {
    const run = withRun([
      statement("UPDATE t SET a = 1 WHERE b", 0, { status: "done", result: affected }),
      statement("SELECT * FROM big", 28, { status: "done", result: truncated }),
      statement("SELECT 3", 47, { status: "skipped" }),
    ]);

    const { rerender } = render(<ResultPanel tab={run} {...props} />);
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual([
      "Result 1",
      "Result 2",
      "Result 3 –",
      "Messages",
    ]);
    expect(screen.getByRole("status")).toHaveTextContent("1 row affected · 2 ms");

    rerender(<ResultPanel tab={{ ...run, shown: 1 }} {...props} />);
    expect(screen.getByRole("status")).toHaveTextContent("Showing the first 2 rows");
    rerender(<ResultPanel tab={{ ...run, shown: 2 }} {...props} />);
    expect(screen.getByRole("status")).toHaveTextContent("an earlier statement failed");
    rerender(<ResultPanel tab={{ ...run, shown: "messages" }} {...props} />);
    expect(screen.getByRole("list", { name: "Messages" })).toHaveTextContent(
      "Running 3 statements",
    );
  });

  it("selects a sub-tab", async () => {
    const onShow = vi.fn();
    render(
      <ResultPanel
        tab={withRun([statement("SELECT 1", 0, { status: "running" })])}
        onShow={onShow}
        onJumpTo={vi.fn()}
      />,
    );

    expect(screen.getByRole("status")).toHaveTextContent("Running");
    await userEvent.click(screen.getByRole("tab", { name: "Messages" }));
    expect(onShow).toHaveBeenCalledWith("messages");
  });

  it("shows a failed statement's error at its location and jumps there", async () => {
    const onJumpTo = vi.fn();
    const error = { code: "syntax", message: 'near "FRM": syntax error' };
    const failed = statement("SELECT * FRM t", 10, { status: "failed", error, errorOffset: 19 });

    render(
      <ResultPanel
        tab={withRun([failed], { text: "SELECT 1;\nSELECT * FRM t" })}
        onShow={vi.fn()}
        onJumpTo={onJumpTo}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "line 2, column 10" }));

    expect(screen.getByRole("alert")).toHaveTextContent('near "FRM": syntax error');
    expect(onJumpTo).toHaveBeenCalledWith(19);
  });

  it("shows a notice and cancelled or pending statements", () => {
    const { rerender } = render(
      <ResultPanel
        tab={tab({ notice: { code: "invalid_argument", message: "There is no statement to run" } })}
        {...props}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("There is no statement to run");
    expect(screen.queryByRole("button")).toBeNull();

    rerender(
      <ResultPanel tab={withRun([statement("SELECT 1", 0, { status: "cancelled" })])} {...props} />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("cancelled");
    rerender(
      <ResultPanel
        tab={withRun([
          statement("SELECT 1", 0, {
            status: "cancelled",
            error: { code: "cancelled", message: "canceling statement" },
          }),
        ])}
        {...props}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("canceling statement");
    rerender(
      <ResultPanel tab={withRun([statement("SELECT 1", 0, { status: "pending" })])} {...props} />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("Waiting");
  });
});
