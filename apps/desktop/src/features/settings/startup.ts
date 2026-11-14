import { create } from "zustand";

import { appReady } from "../../ipc";

type StartupState = { startupMs: number | null; report: () => Promise<void> };

/** The cold start time (NFR-1), measured by the backend once the window is usable. */
export const useStartup = create<StartupState>()((set, get) => ({
  startupMs: null,
  report: async () => {
    if (get().startupMs !== null) return;
    try {
      set({ startupMs: await appReady() });
    } catch {
      // Only a measurement: the app works the same without it.
    }
  },
}));
