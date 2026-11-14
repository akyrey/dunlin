import { create } from "zustand";

import { type AppError, type Settings, settingsGet, settingsSet, toAppError } from "../../ipc";
import { DEFAULT_SETTINGS } from "./defaults";

type SettingsState = {
  settings: Settings;
  loaded: boolean;
  error: AppError | null;
  load: () => Promise<void>;
  /** Saves and adopts `settings`; resolves with the error when the backend refuses them. */
  save: (settings: Settings) => Promise<AppError | null>;
};

export const useSettings = create<SettingsState>()((set) => ({
  settings: DEFAULT_SETTINGS,
  loaded: false,
  error: null,
  load: async () => {
    try {
      set({ settings: await settingsGet(), loaded: true, error: null });
    } catch (caught) {
      // The defaults stay in use; the error is shown in the settings view.
      set({ loaded: true, error: toAppError(caught) });
    }
  },
  save: async (settings) => {
    try {
      set({ settings: await settingsSet(settings), error: null });
      return null;
    } catch (caught) {
      return toAppError(caught);
    }
  },
}));

/** The current settings, outside React. */
export const currentSettings = (): Settings => useSettings.getState().settings;
