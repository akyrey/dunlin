import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";

import {
  connectionDelete,
  connectionList,
  connectionSave,
  connectionTest,
  driverList,
  queryCancel,
  queryExecute,
  sessionClose,
  sessionOpen,
  sqlSplit,
  tabClose,
} from "./commands";
import type { ConnectionDraft } from "./generated";
import { isAppError, toAppError } from "./invoke";

const draft: ConnectionDraft = {
  id: null,
  name: "local",
  driver: "sqlite",
  color: null,
  readOnly: false,
  host: null,
  port: null,
  database: null,
  user: null,
  filePath: "/tmp/a.db",
  options: {},
  tls: { mode: "disable", caFile: null, certFile: null, keyFile: null },
};

afterEach(() => {
  clearMocks();
});

describe("command wrappers", () => {
  it.each([
    ["driver_list", () => driverList(), undefined],
    ["connection_list", () => connectionList(), undefined],
    ["connection_save", () => connectionSave(draft), { args: { profile: draft } }],
    [
      "connection_save",
      () => connectionSave(draft, { dbPassword: "p" }),
      { args: { profile: draft, secrets: { dbPassword: "p" } } },
    ],
    ["query_cancel", () => queryCancel("s1", "t1"), { args: { sessionId: "s1", tabId: "t1" } }],
    ["connection_delete", () => connectionDelete("c1"), { args: { connectionId: "c1" } }],
    ["connection_test", () => connectionTest(draft), { args: { profile: draft } }],
    ["session_open", () => sessionOpen("c1"), { args: { connectionId: "c1" } }],
    ["session_close", () => sessionClose("s1"), { args: { sessionId: "s1" } }],
    [
      "sql_split",
      () => sqlSplit("s1", "SELECT 1"),
      { args: { sessionId: "s1", text: "SELECT 1" } },
    ],
    ["tab_close", () => tabClose("s1", "t1"), { args: { sessionId: "s1", tabId: "t1" } }],
    [
      "query_execute",
      () => queryExecute({ sessionId: "s1", tabId: "t1", sql: "SELECT 1", rowLimit: 10 }),
      { args: { sessionId: "s1", tabId: "t1", sql: "SELECT 1", rowLimit: 10 } },
    ],
  ])("%s sends its arguments in the args envelope", async (command, run, expected) => {
    const calls: { cmd: string; payload: unknown }[] = [];
    mockIPC((cmd, payload) => {
      calls.push({ cmd, payload });
      return null;
    });

    await run();

    expect(calls).toEqual([{ cmd: command, payload: expected ?? {} }]);
  });

  it("rejects with the backend AppError", async () => {
    mockIPC(() => Promise.reject({ code: "syntax", message: "near FRM" }));

    await expect(connectionList()).rejects.toEqual({ code: "syntax", message: "near FRM" });
  });

  it("wraps unexpected rejections as internal errors", async () => {
    mockIPC(() => Promise.reject(new Error("boom")));

    await expect(driverList()).rejects.toEqual({ code: "internal", message: "boom" });
  });
});

describe("toAppError", () => {
  it("keeps AppErrors unchanged and converts strings", () => {
    expect(toAppError({ code: "x", message: "y" })).toEqual({ code: "x", message: "y" });
    expect(toAppError("plain")).toEqual({ code: "internal", message: "plain" });
    expect(isAppError(null)).toBe(false);
    expect(isAppError({ code: 1, message: "y" })).toBe(false);
  });
});
