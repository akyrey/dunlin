import { create } from "zustand";

import { type SessionInfo, sessionClose, sessionOpen } from "../../ipc";

type SessionsState = {
  /** Open sessions by connection id. */
  sessions: Record<string, SessionInfo>;
  activeConnectionId: string | null;
  open: (connectionId: string) => Promise<SessionInfo>;
  close: (connectionId: string) => Promise<void>;
  activate: (connectionId: string | null) => void;
  /** Forgets a session the backend already closed (e.g. its profile was deleted). */
  forget: (connectionId: string) => void;
};

function without<T>(record: Record<string, T>, key: string): Record<string, T> {
  return Object.fromEntries(Object.entries(record).filter(([k]) => k !== key));
}

export const useSessions = create<SessionsState>()((set, get) => ({
  sessions: {},
  activeConnectionId: null,

  open: async (connectionId) => {
    const existing = get().sessions[connectionId];
    if (existing) {
      set({ activeConnectionId: connectionId });
      return existing;
    }
    const session = await sessionOpen(connectionId);
    set({
      sessions: { ...get().sessions, [connectionId]: session },
      activeConnectionId: connectionId,
    });
    return session;
  },

  close: async (connectionId) => {
    const session = get().sessions[connectionId];
    if (!session) return;
    await sessionClose(session.sessionId);
    get().forget(connectionId);
  },

  activate: (connectionId) => {
    set({ activeConnectionId: connectionId });
  },

  forget: (connectionId) => {
    const active = get().activeConnectionId;
    set({
      sessions: without(get().sessions, connectionId),
      activeConnectionId: active === connectionId ? null : active,
    });
  },
}));
