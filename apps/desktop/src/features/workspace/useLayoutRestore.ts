import { useEffect, useState } from "react";

import type { ConnectionProfile } from "../../ipc";
import { useSessions } from "../sessions/sessionsStore";
import { clampSidebar, loadLayout, saveLayout } from "./layout";

type Restore = {
  sidebarWidth: number;
  setSidebarWidth: (width: number) => void;
};

/** Restores the last layout once the profiles are loaded: the active connection reconnects at
 * once, the others are remembered and reconnect when the user opens them. Saves the layout as
 * it changes. */
export function useLayoutRestore(
  profiles: readonly ConnectionProfile[],
  loaded: boolean,
  open: (profile: ConnectionProfile) => Promise<void>,
): Restore {
  const [initial] = useState(loadLayout);
  const [sidebarWidth, setWidth] = useState(initial.sidebarWidth);
  const [restored, setRestored] = useState<boolean>(false);
  const sessions = useSessions((s) => s.sessions);
  const activeConnectionId = useSessions((s) => s.activeConnectionId);
  const remembered = useSessions((s) => s.remembered);

  // Runs once, when the profiles first arrive.
  if (loaded && !restored) {
    setRestored(true);
  }
  useEffect(() => {
    if (!restored) return;
    const known = new Set(profiles.map((p) => p.id));
    const active = initial.activeConnectionId;
    useSessions
      .getState()
      .remember(initial.openConnections.filter((id) => known.has(id) && id !== active));
    const profile = profiles.find((p) => p.id === active);
    if (profile) void open(profile);
    // Only the first load restores; later profile changes must not reopen anything.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [restored]);

  useEffect(() => {
    if (!restored) return;
    const openIds = Object.keys(sessions);
    saveLayout({
      sidebarWidth,
      openConnections: [...openIds, ...remembered.filter((id) => !openIds.includes(id))],
      activeConnectionId,
    });
  }, [restored, sessions, activeConnectionId, remembered, sidebarWidth]);

  return {
    sidebarWidth,
    setSidebarWidth: (width) => {
      setWidth(clampSidebar(width));
    },
  };
}
