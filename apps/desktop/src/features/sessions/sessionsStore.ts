import { create } from "zustand";

import { type SessionInfo, sessionClose, sessionOpen } from "../../ipc";

type SessionsState = {
  /** Open sessions by connection id. */
  sessions: Record<string, SessionInfo>;
  activeConnectionId: string | null;
  /** Why a session ended without the user closing it (e.g. its SSH tunnel dropped). */
  notice: string | null;
  open: (connectionId: string) => Promise<SessionInfo>;
  close: (connectionId: string) => Promise<void>;
  activate: (connectionId: string | null) => void;
  /** Forgets a session the backend already closed (e.g. its profile was deleted). */
  forget: (connectionId: string) => void;
  /** The backend closed this session because its SSH tunnel was lost. */
  markLost: (sessionId: string, message: string) => void;
  dismissNotice: () => void;
};

function without<T>(record: Record<string, T>, key: string): Record<string, T> {
  return Object.fromEntries(Object.entries(record).filter(([k]) => k !== key));
}

export const useSessions = create<SessionsState>()((set, get) => ({
  sessions: {},
  activeConnectionId: null,
  notice: null,

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
      notice: null,
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

  markLost: (sessionId, message) => {
    const entry = Object.entries(get().sessions).find(([, s]) => s.sessionId === sessionId);
    if (!entry) return;
    get().forget(entry[0]);
    set({ notice: `${message}. Open the connection again once the server is reachable.` });
  },

  dismissNotice: () => {
    set({ notice: null });
  },

  forget: (connectionId) => {
    const active = get().activeConnectionId;
    set({
      sessions: without(get().sessions, connectionId),
      activeConnectionId: active === connectionId ? null : active,
    });
  },
}));
