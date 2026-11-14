/** Workspace keyboard shortcuts (task 5.13). Run, run script, cancel and toggle comment live in
 * the editor itself (`SqlEditor`). */
export type Shortcut = "new-tab" | "close-tab" | "next-tab" | "previous-tab" | "focus-results";

export const SHORTCUT_LABELS: Record<Shortcut, string> = {
  "new-tab": "⌘T",
  "close-tab": "⌘W",
  "next-tab": "⌃Tab or ⌥⌘→",
  "previous-tab": "⌃⇧Tab or ⌥⌘←",
  "focus-results": "⌥⌘R",
};

type Keys = Pick<KeyboardEvent, "key" | "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

const isMac = () => typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

export function matchShortcut(event: Keys, mac = isMac()): Shortcut | null {
  const mod = mac ? event.metaKey : event.ctrlKey;
  const key = event.key.toLowerCase();
  if (event.ctrlKey && key === "tab") return event.shiftKey ? "previous-tab" : "next-tab";
  if (!mod) return null;
  if (event.altKey && key === "arrowright") return "next-tab";
  if (event.altKey && key === "arrowleft") return "previous-tab";
  // With Option held, macOS reports a composed character as `key`; `code` is the physical key.
  if (event.altKey && event.code === "KeyR") return "focus-results";
  if (event.altKey || event.shiftKey) return null;
  if (key === "t") return "new-tab";
  if (key === "w") return "close-tab";
  return null;
}
