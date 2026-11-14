/** The app window: knowing when the user asks to quit, and closing it. */
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Calls `ask` when the user closes the window; `ask` returns `true` to keep it open. Returns
 * a function that stops listening. Outside the desktop app there is no window, and nothing
 * happens. */
export async function onQuitRequested(ask: () => boolean): Promise<() => void> {
  if (!isTauri()) return () => undefined;
  return getCurrentWindow().onCloseRequested((event) => {
    if (ask()) event.preventDefault();
  });
}

export function quitApp(): Promise<void> {
  return getCurrentWindow().destroy();
}
