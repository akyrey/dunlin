import { create } from "zustand";

import {
  type AppError,
  type ConnectionDraft,
  type ConnectionProfile,
  type DriverInfo,
  connectionDelete,
  connectionList,
  connectionSave,
  driverList,
  toAppError,
} from "../../ipc";

type ConnectionsState = {
  drivers: DriverInfo[];
  profiles: ConnectionProfile[];
  loaded: boolean;
  error: AppError | null;
  load: () => Promise<void>;
  save: (draft: ConnectionDraft) => Promise<ConnectionProfile>;
  remove: (connectionId: string) => Promise<void>;
};

const byName = (a: ConnectionProfile, b: ConnectionProfile) =>
  a.name.localeCompare(b.name, undefined, { sensitivity: "base" });

export const useConnections = create<ConnectionsState>()((set, get) => ({
  drivers: [],
  profiles: [],
  loaded: false,
  error: null,

  load: async () => {
    try {
      const [drivers, profiles] = await Promise.all([driverList(), connectionList()]);
      set({ drivers, profiles, loaded: true, error: null });
    } catch (error) {
      set({ loaded: true, error: toAppError(error) });
    }
  },

  save: async (draft) => {
    const saved = await connectionSave(draft);
    const others = get().profiles.filter((p) => p.id !== saved.id);
    set({ profiles: [...others, saved].sort(byName) });
    return saved;
  },

  remove: async (connectionId) => {
    await connectionDelete(connectionId);
    set({ profiles: get().profiles.filter((p) => p.id !== connectionId) });
  },
}));
