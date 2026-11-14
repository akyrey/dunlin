import { DEFAULT_LAYOUT, SIDEBAR_WIDTH, clampSidebar, loadLayout, saveLayout } from "./layout";

describe("layout", () => {
  it("round-trips through storage", () => {
    const layout = { sidebarWidth: 300, openConnections: ["a", "b"], activeConnectionId: "a" };
    saveLayout(layout);
    expect(loadLayout()).toEqual(layout);
  });

  it("falls back to the defaults for missing or corrupt data", () => {
    expect(loadLayout()).toEqual(DEFAULT_LAYOUT);
    localStorage.setItem("dunlin.layout", "{not json");
    expect(loadLayout()).toEqual(DEFAULT_LAYOUT);
    localStorage.setItem(
      "dunlin.layout",
      JSON.stringify({ sidebarWidth: "wide", openConnections: [1, "x"], activeConnectionId: 3 }),
    );
    expect(loadLayout()).toEqual({
      sidebarWidth: SIDEBAR_WIDTH.default,
      openConnections: ["x"],
      activeConnectionId: null,
    });
  });

  it("keeps the sidebar within its bounds", () => {
    expect(clampSidebar(10)).toBe(SIDEBAR_WIDTH.min);
    expect(clampSidebar(10_000)).toBe(SIDEBAR_WIDTH.max);
    expect(clampSidebar(300.4)).toBe(300);
  });
});
