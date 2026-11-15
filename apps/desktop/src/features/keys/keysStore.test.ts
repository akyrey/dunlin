import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";

import type { KeySummary, ScanPage } from "../../ipc";
import { listOf, useKeys } from "./keysStore";

const key = (name: string): KeySummary => ({ key: { name, id: btoa(name) }, keyType: "string" });

/** Answers each `kv_scan` with the next page, and records the cursors asked for. */
function scans(pages: (ScanPage | Error)[]) {
  const cursors: (string | undefined)[] = [];
  mockIPC((cmd, payload) => {
    if (cmd !== "kv_scan") throw new Error(`unexpected ${cmd}`);
    const args = (payload as { args: { cursor?: string } }).args;
    cursors.push(args.cursor);
    const next = pages.shift();
    if (next instanceof Error) return Promise.reject({ code: "connect", message: next.message });
    return next ?? { keys: [], cursor: null };
  });
  return cursors;
}

beforeEach(() => {
  useKeys.setState({ lists: {} });
});

afterEach(() => {
  clearMocks();
});

describe("useKeys", () => {
  it("steps past empty SCAN pages until keys arrive", async () => {
    const cursors = scans([
      { keys: [], cursor: "7" },
      { keys: [], cursor: "9" },
      { keys: [key("a")], cursor: "12" },
    ]);

    await useKeys.getState().search("s1", "  user:*  ");

    const list = listOf(useKeys.getState(), "s1");
    expect(cursors).toEqual([undefined, "7", "9"]);
    expect(list.pattern).toBe("user:*");
    expect(list.keys.map((k) => k.key.name)).toEqual(["a"]);
    expect(list.cursor).toBe("12");
    expect(list.loading).toBe(false);
  });

  it("continues from the cursor, drops keys seen twice, and stops when complete", async () => {
    const cursors = scans([
      { keys: [key("a"), key("b")], cursor: "5" },
      { keys: [key("b"), key("c")], cursor: null },
    ]);

    await useKeys.getState().search("s1", "");
    await useKeys.getState().more("s1");
    await useKeys.getState().more("s1");

    const list = listOf(useKeys.getState(), "s1");
    expect(list.pattern).toBe("*");
    expect(list.keys.map((k) => k.key.name)).toEqual(["a", "b", "c"]);
    expect(list.cursor).toBeNull();
    expect(cursors).toEqual([undefined, "5"]);
  });

  it("keeps the error of a failed step", async () => {
    scans([new Error("Connection refused")]);

    await useKeys.getState().search("s1", "*");

    const list = listOf(useKeys.getState(), "s1");
    expect(list.error?.message).toBe("Connection refused");
    expect(list.loading).toBe(false);
  });

  it("drops the answer to a pattern that was replaced while it ran", async () => {
    let release: (page: ScanPage) => void = () => undefined;
    mockIPC((_cmd, payload) => {
      const pattern = (payload as { args: { pattern: string } }).args.pattern;
      if (pattern === "old:*") {
        return new Promise<ScanPage>((resolve) => {
          release = resolve;
        });
      }
      return { keys: [key("new:1")], cursor: null };
    });

    const old = useKeys.getState().search("s1", "old:*");
    await useKeys.getState().search("s1", "new:*");
    release({ keys: [key("old:1")], cursor: null });
    await old;

    const list = listOf(useKeys.getState(), "s1");
    expect(list.pattern).toBe("new:*");
    expect(list.keys.map((k) => k.key.name)).toEqual(["new:1"]);
  });

  it("starts every session from the full pattern", () => {
    expect(listOf(useKeys.getState(), "unknown")).toMatchObject({ pattern: "*", keys: [] });
  });
});
