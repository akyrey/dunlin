import { useWorkspace } from "../workspace/workspaceStore";
import { useEditor } from "./editorStore";
import { loadTabs, persistTabs, saveTabs, snapshot } from "./persistence";

beforeEach(() => {
  useEditor.setState({ tabs: {} });
  useWorkspace.setState({ workspaces: {} });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("tab persistence", () => {
  it("round-trips saved tabs and ignores unreadable entries", () => {
    saveTabs("c1", [{ title: "a", text: "SELECT 1", rowLimit: 100 }]);
    localStorage.setItem("dunlin.tabs.c2", "not json");
    localStorage.setItem(
      "dunlin.tabs.c3",
      JSON.stringify([{ title: 1 }, { title: "ok", text: "", rowLimit: 5 }]),
    );

    expect(loadTabs("c1")).toEqual([{ title: "a", text: "SELECT 1", rowLimit: 100 }]);
    expect(loadTabs("c2")).toEqual([]);
    expect(loadTabs("c3")).toEqual([{ title: "ok", text: "", rowLimit: 5 }]);
    expect(loadTabs("missing")).toEqual([]);
  });

  it("snapshots tabs in tab-strip order, and an empty list for a session without tabs", () => {
    const a = useEditor.getState().openTab("s1", { title: "a" });
    useEditor.getState().openTab("s1", { title: "b" });
    useWorkspace.getState().move("s1", a, 9);

    const saved = snapshot(useEditor.getState().tabs, [
      { sessionId: "s1", connectionId: "c1" },
      { sessionId: "s2", connectionId: "c2" },
    ]);

    expect(saved.get("c1")?.map((t) => t.title)).toEqual(["b", "a"]);
    expect(saved.get("c2")).toEqual([]);
  });

  it("saves shortly after a change and stops when asked", () => {
    vi.useFakeTimers();
    const stop = persistTabs(() => [{ sessionId: "s1", connectionId: "c1" }]);

    const tabId = useEditor.getState().openTab("s1");
    useEditor.getState().setText(tabId, "SELECT 42");
    vi.advanceTimersByTime(500);
    expect(loadTabs("c1")).toEqual([{ title: "Query 1", text: "SELECT 42", rowLimit: 1000 }]);

    stop();
    useEditor.getState().setText(tabId, "changed");
    vi.advanceTimersByTime(500);
    expect(loadTabs("c1")[0]?.text).toBe("SELECT 42");
  });
});
