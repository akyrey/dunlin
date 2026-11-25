/** Which session's query statistics dialog is open (Phase 10). */
import { create } from "zustand";

type QueryStatsState = {
  openFor: string | null;
  open: (sessionId: string) => void;
  close: () => void;
};

export const useQueryStats = create<QueryStatsState>()((set) => ({
  openFor: null,
  open: (sessionId) => {
    set({ openFor: sessionId });
  },
  close: () => {
    set({ openFor: null });
  },
}));
