/** What the window looked like when the app last ran (task 8.12): the sidebar width and which
 * connections were open. Kept in localStorage; window size and position are kept by Tauri's
 * window-state plugin. */

const KEY = "dunlin.layout";

export const SIDEBAR_WIDTH = { min: 180, max: 480, default: 256 } as const;

export type Layout = {
  sidebarWidth: number;
  /** Connections open at the last save, the active one first. */
  openConnections: string[];
  activeConnectionId: string | null;
};

export const DEFAULT_LAYOUT: Layout = {
  sidebarWidth: SIDEBAR_WIDTH.default,
  openConnections: [],
  activeConnectionId: null,
};

export function clampSidebar(width: number): number {
  if (!Number.isFinite(width)) return SIDEBAR_WIDTH.default;
  return Math.round(Math.min(SIDEBAR_WIDTH.max, Math.max(SIDEBAR_WIDTH.min, width)));
}

export function loadLayout(storage: Storage = localStorage): Layout {
  try {
    const parsed: unknown = JSON.parse(storage.getItem(KEY) ?? "null");
    if (typeof parsed !== "object" || parsed === null) return DEFAULT_LAYOUT;
    const raw = parsed as Record<string, unknown>;
    const open = Array.isArray(raw.openConnections)
      ? raw.openConnections.filter((id): id is string => typeof id === "string")
      : [];
    return {
      sidebarWidth: clampSidebar(Number(raw.sidebarWidth)),
      openConnections: open,
      activeConnectionId:
        typeof raw.activeConnectionId === "string" ? raw.activeConnectionId : null,
    };
  } catch {
    // An unreadable layout only costs the user the previous arrangement.
    return DEFAULT_LAYOUT;
  }
}

export function saveLayout(layout: Layout, storage: Storage = localStorage): void {
  try {
    storage.setItem(KEY, JSON.stringify(layout));
  } catch {
    // Storage full or unavailable: the layout is a convenience, the app works without it.
  }
}
