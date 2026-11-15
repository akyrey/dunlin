import type { QueryExecuteArgs, QueryResult, StatementRange } from "../../ipc";
import * as ipc from "../../ipc";
import { useSchema } from "../schema/schemaStore";
import { useSessions } from "../sessions/sessionsStore";
import { useWorkspace, workspaceOf } from "../workspace/workspaceStore";
import { useCompletion } from "./completionStore";
import { DEFAULT_SETTINGS } from "../settings/defaults";
import { useSettings } from "../settings/settingsStore";
import { shownResult, tabsOf, useEditor } from "./editorStore";

const DEFAULT_ROW_LIMIT = DEFAULT_SETTINGS.defaultRowLimit;

vi.mock("../../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../ipc")>();
  return {
    ...actual,
    queryExecute: vi.fn(),
    sqlSplit: vi.fn(),
    queryCancel: vi.fn(),
    tabClose: vi.fn(),
  };
});

const queryExecute = vi.mocked(ipc.queryExecute);
const sqlSplit = vi.mocked(ipc.sqlSplit);
const queryCancel = vi.mocked(ipc.queryCancel);
const tabClose = vi.mocked(ipc.tabClose);

const rows: QueryResult = {
  outcome: { kind: "rows", columns: [], rows: [], truncated: false },
  elapsedMs: 1,
  statementClass: "read",
  source: null,
};

const SESSION = "s1";
const SCRIPT = "SELECT 1; SELECT 2; SELECT 3";

/** Splits on `;` like the backend, flagging `DELETE FROM x` without WHERE. */
function split(text: string): StatementRange[] {
  const ranges: StatementRange[] = [];
  let start = 0;
  for (const part of text.split(";")) {
    const leading = part.length - part.trimStart().length;
    const trimmed = part.trim();
    if (trimmed) {
      const danger = /^delete from \w+$/i.test(trimmed)
        ? "DELETE without WHERE affects every row"
        : null;
      ranges.push({ start: start + leading, end: start + leading + trimmed.length, danger });
    }
    start += part.length + 1;
  }
  return ranges;
}

let tabId = "";
const tab = () => {
  const state = useEditor.getState().tabs[tabId];
  if (!state) throw new Error("no tab");
  return state;
};
const statuses = () => tab().run?.statements.map((s) => s.status);
const executed = () => queryExecute.mock.calls.map(([args]) => args.sql);
const cursor = (head: number) => ({ from: head, to: head, head });

beforeEach(() => {
  useEditor.setState({ tabs: {} });
  useWorkspace.setState({ workspaces: {} });
  for (const mock of [queryExecute, sqlSplit, queryCancel, tabClose]) mock.mockReset();
  sqlSplit.mockImplementation((_session, text) => Promise.resolve(split(text)));
  queryExecute.mockResolvedValue(rows);
  tabId = useEditor.getState().openTab(SESSION);
});

describe("editor tabs", () => {
  it("numbers new tabs and adds them to the workspace", () => {
    const second = useEditor.getState().openTab(SESSION);
    const named = useEditor
      .getState()
      .openTab(SESSION, { title: "people", text: "SELECT 1", rowLimit: 5 });

    expect(tabsOf(useEditor.getState(), SESSION).map((t) => t.title)).toEqual([
      "Query 1",
      "Query 2",
      "people",
    ]);
    expect(useEditor.getState().tabs[named]).toMatchObject({ text: "SELECT 1", rowLimit: 5 });
    expect(workspaceOf(useWorkspace.getState(), SESSION).views.map((v) => v.key)).toEqual([
      tabId,
      second,
      named,
    ]);
    expect(tab().rowLimit).toBe(DEFAULT_ROW_LIMIT);
  });

  it("gives a new tab the row limit from the settings", () => {
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, defaultRowLimit: 250 } });
    const opened = useEditor.getState().openTab(SESSION);
    useSettings.setState({ settings: DEFAULT_SETTINGS });

    expect(useEditor.getState().tabs[opened]?.rowLimit).toBe(250);
  });

  it("renames, sets valid row limits only, and inserts text with a remount", () => {
    useEditor.getState().rename(tabId, "  mine ");
    useEditor.getState().rename(tabId, "  ");
    useEditor.getState().setRowLimit(tabId, 10_000);
    useEditor.getState().setRowLimit(tabId, 0);
    useEditor.getState().setRowLimit(tabId, 1.5);
    useEditor.getState().insertText(tabId, "SELECT 1;");
    useEditor.getState().insertText(tabId, "SELECT 2;");

    expect(tab()).toMatchObject({
      title: "mine",
      rowLimit: 10_000,
      text: "SELECT 1;\n\nSELECT 2;",
      revision: 2,
    });
  });

  it("closes a tab and its backend connection, tolerating a session already gone", async () => {
    tabClose.mockRejectedValueOnce({ code: "not_found", message: "The session was not found" });

    await useEditor.getState().closeTab(tabId);

    expect(tabClose).toHaveBeenCalledWith(SESSION, tabId);
    expect(useEditor.getState().tabs).toEqual({});
    expect(workspaceOf(useWorkspace.getState(), SESSION).views).toEqual([]);
  });

  it("reports other failures to close as a notice", async () => {
    tabClose.mockRejectedValueOnce({ code: "internal", message: "boom" });

    await useEditor.getState().closeTab(tabId);

    expect(useSessions.getState().notice).toContain("boom");
  });

  it("caches the split per text", async () => {
    useEditor.getState().setText(tabId, SCRIPT);

    await useEditor.getState().updateSplit(tabId);
    await useEditor.getState().updateSplit(tabId);

    expect(sqlSplit).toHaveBeenCalledTimes(1);
    expect(tab().split?.ranges).toHaveLength(3);
  });
});

describe("running", () => {
  it("runs only the statement under the cursor", async () => {
    useEditor.getState().setText(tabId, SCRIPT);

    await useEditor.getState().run(tabId, cursor(13), "statement");

    expect(executed()).toEqual(["SELECT 2"]);
    expect(queryExecute.mock.calls[0]?.[0]).toMatchObject<Partial<QueryExecuteArgs>>({
      tabId,
      rowLimit: DEFAULT_ROW_LIMIT,
    });
    expect(shownResult(tab())).toEqual(rows);
  });

  it("runs every statement of the selection", async () => {
    useEditor.getState().setText(tabId, SCRIPT);

    await useEditor.getState().run(tabId, { from: 10, to: 28, head: 28 }, "statement");

    expect(executed()).toEqual(["SELECT 2", "SELECT 3"]);
    expect(tab().run?.statements.map((s) => s.start)).toEqual([10, 20]);
  });

  it("runs a script in order and stops at the first error, marking its position", async () => {
    useEditor.getState().setText(tabId, "SELECT 1; SELECT * FRM t; SELECT 3");
    queryExecute.mockResolvedValueOnce(rows).mockRejectedValueOnce({
      code: "syntax",
      message: 'near "FRM": syntax error',
      detail: { position: 9 },
    });

    await useEditor.getState().run(tabId, cursor(0), "script");

    expect(executed()).toEqual(["SELECT 1", "SELECT * FRM t"]);
    expect(statuses()).toEqual(["done", "failed", "skipped"]);
    expect(tab().run?.statements[1]?.errorOffset).toBe(19);
    expect(tab()).toMatchObject({ running: false, shown: 1 });
    expect(tab().run?.messages.map((m) => m.text)).toEqual([
      "Running 3 statements",
      "Statement 1: 0 rows · 1 ms",
      'Statement 2: near "FRM": syntax error',
    ]);
  });

  it("reports an empty editor and split failures without running", async () => {
    await useEditor.getState().run(tabId, cursor(0), "statement");
    expect(tab().notice?.message).toBe("There is no statement to run");

    useEditor.getState().setText(tabId, "SELECT 1");
    sqlSplit.mockRejectedValueOnce({ code: "not_found", message: "The session was not found" });
    await useEditor.getState().run(tabId, cursor(0), "statement");

    expect(tab().notice?.code).toBe("not_found");
    expect(queryExecute).not.toHaveBeenCalled();
  });

  it("asks once for every dangerous statement of a script, then confirms only those", async () => {
    useEditor.getState().setText(tabId, "SELECT 1; DELETE FROM a; DELETE FROM b");

    await useEditor.getState().run(tabId, cursor(0), "script");
    expect(queryExecute).not.toHaveBeenCalled();
    expect(tab().confirmation?.statements.filter((s) => s.danger)).toHaveLength(2);

    await useEditor.getState().confirm(tabId);
    expect(queryExecute.mock.calls.map(([args]) => [args.sql, args.confirmed])).toEqual([
      ["SELECT 1", undefined],
      ["DELETE FROM a", true],
      ["DELETE FROM b", true],
    ]);
    expect(tab().confirmation).toBeNull();
  });

  it("can dismiss a confirmation", async () => {
    useEditor.getState().setText(tabId, "DELETE FROM a");
    await useEditor.getState().run(tabId, cursor(0), "statement");

    useEditor.getState().dismissConfirmation(tabId);
    await useEditor.getState().confirm(tabId);

    expect(tab().confirmation).toBeNull();
    expect(queryExecute).not.toHaveBeenCalled();
  });

  it("asks when the backend finds a danger the split missed, and resumes there", async () => {
    useEditor.getState().setText(tabId, "SELECT 1; TRUNCATE t; SELECT 3");
    queryExecute.mockResolvedValueOnce(rows).mockRejectedValueOnce({
      code: "confirmation_required",
      message: "TRUNCATE removes every row",
    });

    await useEditor.getState().run(tabId, cursor(0), "script");
    expect(tab().confirmation?.statements.map((s) => [s.sql, s.danger])).toEqual([
      ["TRUNCATE t", "TRUNCATE removes every row"],
      ["SELECT 3", null],
    ]);

    await useEditor.getState().confirm(tabId);
    expect(executed()).toEqual(["SELECT 1", "TRUNCATE t", "TRUNCATE t", "SELECT 3"]);
    expect(queryExecute.mock.calls[2]?.[0].confirmed).toBe(true);
  });

  it("cancels the running statement and skips the rest", async () => {
    useEditor.getState().setText(tabId, SCRIPT);
    let finish: () => void = () => undefined;
    queryExecute.mockReturnValueOnce(
      new Promise((resolve) => {
        finish = () => {
          resolve(rows);
        };
      }),
    );

    const running = useEditor.getState().run(tabId, cursor(0), "script");
    await vi.waitFor(() => {
      expect(tab().running).toBe(true);
    });
    expect(tab().startedAt).not.toBeNull();
    await useEditor.getState().cancel(tabId);
    finish();
    await running;

    expect(queryCancel).toHaveBeenCalledWith(SESSION, tabId);
    expect(statuses()).toEqual(["done", "cancelled", "cancelled"]);
    expect(executed()).toEqual(["SELECT 1"]);
  });

  it("marks a cancelled statement and reports a failed cancel", async () => {
    useEditor.getState().setText(tabId, SCRIPT);
    queryExecute.mockRejectedValueOnce({ code: "cancelled", message: "canceling statement" });
    await useEditor.getState().run(tabId, cursor(0), "script");
    expect(statuses()).toEqual(["cancelled", "cancelled", "cancelled"]);

    await useEditor.getState().cancel(tabId);
    expect(queryCancel).not.toHaveBeenCalled();
  });

  it("reloads the schema tree and autocomplete after DDL", async () => {
    const reloadTree = vi.fn(() => Promise.resolve());
    const reloadCompletion = vi.fn(() => Promise.resolve());
    useSchema.setState({ reload: reloadTree });
    useCompletion.setState({ reload: reloadCompletion });
    queryExecute.mockResolvedValueOnce({ ...rows, statementClass: "ddl" });
    useEditor.getState().setText(tabId, "CREATE TABLE t (a int)");

    await useEditor.getState().run(tabId, cursor(0), "statement");

    expect(reloadTree).toHaveBeenCalledWith(SESSION);
    expect(reloadCompletion).toHaveBeenCalledWith(SESSION);
  });

  it("drops the session's tabs when its tunnel is lost", async () => {
    const markLost = vi.fn();
    useSessions.setState({ markLost });
    useEditor.getState().openTab("other");
    queryExecute.mockRejectedValueOnce({
      code: "tunnel_lost",
      message: "The SSH connection was lost",
    });
    useEditor.getState().setText(tabId, "SELECT 1");

    await useEditor.getState().run(tabId, cursor(0), "statement");

    expect(markLost).toHaveBeenCalledWith(SESSION, "The SSH connection was lost");
    expect(Object.values(useEditor.getState().tabs).map((t) => t.sessionId)).toEqual(["other"]);
  });

  it("drops every tab of a closed session", () => {
    useEditor.getState().dropSession(SESSION);

    expect(useEditor.getState().tabs).toEqual({});
  });
});
