/** Which views have pending edits (for the close and quit guards, task 7.13), and which
 * sessions may apply without the preview (task 7.11). */
import { create } from "zustand";

type EditState = {
  /** By workspace view key. */
  pending: Record<string, { sessionId: string; count: number }>;
  /** Sessions where the user chose to skip the preview after seeing it once. */
  previewSkipped: Record<string, true>;
  setPending: (viewKey: string, sessionId: string, count: number) => void;
  skipPreview: (sessionId: string) => void;
};

export const useEdits = create<EditState>()((set, get) => ({
  pending: {},
  previewSkipped: {},

  setPending: (viewKey, sessionId, count) => {
    const rest = Object.fromEntries(Object.entries(get().pending).filter(([k]) => k !== viewKey));
    set({ pending: count > 0 ? { ...rest, [viewKey]: { sessionId, count } } : rest });
  },

  skipPreview: (sessionId) => {
    set({ previewSkipped: { ...get().previewSkipped, [sessionId]: true } });
  },
}));

/** Views with pending edits: all of them, or one session's. */
export function pendingViews(state: Pick<EditState, "pending">, sessionId?: string): string[] {
  return Object.entries(state.pending)
    .filter(([, v]) => sessionId === undefined || v.sessionId === sessionId)
    .map(([k]) => k);
}
