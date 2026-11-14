import { create } from "zustand";

import {
  type AppError,
  type ConnectionDraft,
  type ConnectionProfile,
  type ConnectionSecrets,
  type DriverInfo,
  connectionDelete,
  connectionDuplicate,
  connectionList,
  connectionReorder,
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
  save: (draft: ConnectionDraft, secrets?: ConnectionSecrets) => Promise<ConnectionProfile>;
  remove: (connectionId: string) => Promise<void>;
  duplicate: (connectionId: string) => Promise<ConnectionProfile>;
  /** Moves a connection one place up or down among those in its folder. */
  move: (connectionId: string, step: -1 | 1) => Promise<void>;
};

/** The ids in the order after moving `id` past its neighbour in the same folder, or `null` when
 * it is already at that end. */
export function movedOrder(
  profiles: readonly ConnectionProfile[],
  id: string,
  step: -1 | 1,
): string[] | null {
  const index = profiles.findIndex((p) => p.id === id);
  const moving = profiles[index];
  if (!moving) return null;
  let other = index + step;
  while (profiles[other] && profiles[other]?.folder !== moving.folder) other += step;
  const neighbour = profiles[other];
  if (!neighbour) return null;
  const ids = profiles.map((p) => p.id);
  ids[index] = neighbour.id;
  ids[other] = moving.id;
  return ids;
}

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

  save: async (draft, secrets) => {
    const saved = await connectionSave(draft, secrets);
    const profiles = get().profiles;
    // Kept in its place when edited; a new connection goes last, as the backend lists it.
    set({
      profiles: profiles.some((p) => p.id === saved.id)
        ? profiles.map((p) => (p.id === saved.id ? saved : p))
        : [...profiles, saved],
    });
    return saved;
  },

  remove: async (connectionId) => {
    await connectionDelete(connectionId);
    set({ profiles: get().profiles.filter((p) => p.id !== connectionId) });
  },

  duplicate: async (connectionId) => {
    const copy = await connectionDuplicate(connectionId);
    // The backend placed it after the original; reloading the list shows exactly that.
    set({ profiles: await connectionList() });
    return copy;
  },

  move: async (connectionId, step) => {
    const order = movedOrder(get().profiles, connectionId, step);
    if (!order) return;
    set({ profiles: await connectionReorder(order) });
  },
}));
