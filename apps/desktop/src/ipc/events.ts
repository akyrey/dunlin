/** Events the backend sends without being asked. */
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { SessionHealthEvent } from "./generated";

/** Calls `handle` when an open session stops or starts answering (task 8.15). Returns a
 * function that stops listening; outside the desktop app nothing is sent. */
export async function onSessionHealth(
  handle: (event: SessionHealthEvent) => void,
): Promise<() => void> {
  if (!isTauri()) return () => undefined;
  return listen<SessionHealthEvent>("session_health", (event) => {
    handle(event.payload);
  });
}
