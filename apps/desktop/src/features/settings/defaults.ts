import type { Settings } from "../../ipc";

/** Mirrors `Settings::default()` in dbc-store; used until the backend's settings arrive. */
export const DEFAULT_SETTINGS: Settings = {
  theme: "system",
  editorFontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
  editorFontSize: 13,
  defaultRowLimit: 1000,
  defaultPageSize: 200,
  historyCap: 10_000,
  previewPolicy: "once_per_session",
  nullText: "NULL",
  checkForUpdates: true,
  reconnectOnStart: true,
};

/** Bounds checked again by the backend (`Settings::validate`). */
export const BOUNDS = {
  editorFontSize: { min: 9, max: 32 },
  defaultRowLimit: { min: 1, max: 100_000 },
  defaultPageSize: { min: 10, max: 10_000 },
  historyCap: { min: 0, max: 1_000_000 },
  nullTextChars: 20,
} as const;
