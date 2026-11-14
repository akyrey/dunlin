import type { QueryResult } from "../../ipc";
import * as ipc from "../../ipc";
import { useSchema } from "../schema/schemaStore";
import { DEFAULT_ROW_LIMIT, useEditor } from "./editorStore";

vi.mock("../../ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../ipc")>();
  return { ...actual, queryExecute: vi.fn(), sqlSplit: vi.fn(), queryCancel: vi.fn() };
});

const queryExecute = vi.mocked(ipc.queryExecute);
const sqlSplit = vi.mocked(ipc.sqlSplit);
const queryCancel = vi.mocked(ipc.queryCancel);

const rowsResult: QueryResult = {
  outcome: { kind: "rows", columns: [], rows: [], truncated: false },
  elapsedMs: 1,
  statementClass: "read",
};

const SESSION = "s1";

function tab() {
  const state = useEditor.getState().tabs[SESSION];
  if (!state) throw new Error("no tab");
  return state;
}

beforeEach(() => {
  useEditor.setState({ tabs: {} });
  queryExecute.mockReset();
  sqlSplit.mockReset();
  queryCancel.mockReset();
  useEditor.getState().ensureTab(SESSION);
});

describe("editor store cancellation", () => {
  it("records when a statement starts and clears it when it ends", async () => {
    let finish: (result: QueryResult) => void = () => undefined;
    queryExecute.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    useEditor.getState().setText(SESSION, "SELECT 1");

    const running = useEditor.getState().run(SESSION, { from: 0, to: 8, head: 8 });
    const startedAt = tab().startedAt;
    finish(rowsResult);
    await running;

    expect(typeof startedAt).toBe("number");
    expect(tab().startedAt).toBeNull();
  });

  it("cancels only a running statement", async () => {
    await useEditor.getState().cancel(SESSION);
    expect(queryCancel).not.toHaveBeenCalled();

    useEditor.setState({ tabs: { [SESSION]: { ...tab(), running: true } } });
    await useEditor.getState().cancel(SESSION);

    expect(queryCancel).toHaveBeenCalledWith(SESSION, tab().tabId);
  });

  it("reports a failed cancel", async () => {
    useEditor.setState({ tabs: { [SESSION]: { ...tab(), running: true } } });
    queryCancel.mockRejectedValue({ code: "connect", message: "cannot reach server" });

    await useEditor.getState().cancel(SESSION);

    expect(tab().error?.error.message).toBe("cannot reach server");
  });
});

describe("editor store", () => {
  it("creates one tab per session and keeps it", () => {
    const first = tab().tabId;

    useEditor.getState().ensureTab(SESSION);

    expect(tab().tabId).toBe(first);
  });

  it("runs the selection when there is one", async () => {
    useEditor.getState().setText(SESSION, "SELECT 1; SELECT 2");
    queryExecute.mockResolvedValue(rowsResult);

    await useEditor.getState().run(SESSION, { from: 10, to: 18, head: 18 });

    expect(queryExecute).toHaveBeenCalledWith({
      sessionId: SESSION,
      tabId: tab().tabId,
      sql: "SELECT 2",
      rowLimit: DEFAULT_ROW_LIMIT,
      confirmed: false,
    });
    expect(tab().result).toEqual(rowsResult);
    expect(sqlSplit).not.toHaveBeenCalled();
  });

  it("runs the statement under the cursor otherwise", async () => {
    useEditor.getState().setText(SESSION, "SELECT 1;\nSELECT 2;");
    sqlSplit.mockResolvedValue([
      { start: 0, end: 8 },
      { start: 10, end: 18 },
    ]);
    queryExecute.mockResolvedValue(rowsResult);

    await useEditor.getState().run(SESSION, { from: 12, to: 12, head: 12 });

    expect(queryExecute.mock.calls[0]?.[0].sql).toBe("SELECT 2");
  });

  it("reports an empty editor without calling the backend", async () => {
    sqlSplit.mockResolvedValue([]);

    await useEditor.getState().run(SESSION, { from: 0, to: 0, head: 0 });

    expect(tab().error?.error.code).toBe("invalid_argument");
    expect(queryExecute).not.toHaveBeenCalled();
  });

  it("reports a blank selection", async () => {
    useEditor.getState().setText(SESSION, "   ");

    await useEditor.getState().run(SESSION, { from: 0, to: 3, head: 3 });

    expect(tab().error?.error.code).toBe("invalid_argument");
  });

  it("turns a server position into an absolute editor offset", async () => {
    useEditor.getState().setText(SESSION, "SELECT 1;\nSELECT * FRM t");
    sqlSplit.mockResolvedValue([
      { start: 0, end: 8 },
      { start: 10, end: 24 },
    ]);
    queryExecute.mockRejectedValue({
      code: "syntax",
      message: "near FRM",
      detail: { position: 9 },
    });

    await useEditor.getState().run(SESSION, { from: 20, to: 20, head: 20 });

    expect(tab().error).toEqual({
      error: { code: "syntax", message: "near FRM", detail: { position: 9 } },
      offset: 19,
    });
    expect(tab().running).toBe(false);
  });

  it("keeps errors without a position", async () => {
    useEditor.getState().setText(SESSION, "SELECT 1");
    queryExecute.mockRejectedValue({ code: "read_only", message: "read-only" });

    await useEditor.getState().run(SESSION, { from: 0, to: 8, head: 8 });

    expect(tab().error?.offset).toBeNull();
  });

  it("reports split failures", async () => {
    useEditor.getState().setText(SESSION, "SELECT 1");
    sqlSplit.mockRejectedValue({ code: "not_found", message: "The session was not found" });

    await useEditor.getState().run(SESSION, { from: 0, to: 0, head: 0 });

    expect(tab().error?.error.code).toBe("not_found");
  });

  it("asks for confirmation and reruns with confirmed: true", async () => {
    useEditor.getState().setText(SESSION, "DELETE FROM t");
    queryExecute.mockRejectedValueOnce({ code: "confirmation_required", message: "every row" });
    queryExecute.mockResolvedValueOnce(rowsResult);

    await useEditor.getState().run(SESSION, { from: 0, to: 13, head: 13 });
    expect(tab().confirmation).toEqual({ reason: "every row", sql: "DELETE FROM t", start: 0 });

    await useEditor.getState().confirm(SESSION);

    expect(queryExecute.mock.calls[1]?.[0].confirmed).toBe(true);
    expect(tab().confirmation).toBeNull();
    expect(tab().result).toEqual(rowsResult);
  });

  it("can dismiss a confirmation without running", async () => {
    useEditor.setState({
      tabs: {
        [SESSION]: { ...tab(), confirmation: { reason: "x", sql: "DROP TABLE t", start: 0 } },
      },
    });

    useEditor.getState().dismissConfirmation(SESSION);
    await useEditor.getState().confirm(SESSION);

    expect(tab().confirmation).toBeNull();
    expect(queryExecute).not.toHaveBeenCalled();
  });

  it("ignores runs while a statement is running and for unknown sessions", async () => {
    useEditor.setState({ tabs: { [SESSION]: { ...tab(), text: "SELECT 1", running: true } } });

    await useEditor.getState().run(SESSION, { from: 0, to: 8, head: 8 });
    await useEditor.getState().run("other", { from: 0, to: 8, head: 8 });

    expect(queryExecute).not.toHaveBeenCalled();
  });

  it("inserts text after what the editor holds and remounts it", () => {
    useEditor.getState().insertText(SESSION, "SELECT 1;");
    expect(tab()).toMatchObject({ text: "SELECT 1;", revision: 1 });

    useEditor.getState().insertText(SESSION, "SELECT 2;");
    expect(tab()).toMatchObject({ text: "SELECT 1;\n\nSELECT 2;", revision: 2 });
  });

  it("reloads the schema tree after DDL, not after reads", async () => {
    const reload = vi.spyOn(useSchema.getState(), "reload").mockResolvedValue(undefined);
    useSchema.setState({ reload });
    sqlSplit.mockResolvedValue([{ start: 0, end: 8 }]);
    queryExecute.mockResolvedValueOnce(rowsResult);
    queryExecute.mockResolvedValueOnce({ ...rowsResult, statementClass: "ddl" });
    useEditor.getState().setText(SESSION, "SELECT 1");

    await useEditor.getState().run(SESSION, { from: 0, to: 0, head: 0 });
    expect(reload).not.toHaveBeenCalled();
    await useEditor.getState().run(SESSION, { from: 0, to: 0, head: 0 });
    expect(reload).toHaveBeenCalledWith(SESSION);
  });

  it("drops a tab", () => {
    useEditor.getState().dropTab(SESSION);

    expect(useEditor.getState().tabs).toEqual({});
  });
});
