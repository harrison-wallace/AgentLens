import { create } from "zustand";
import { watcherStatus } from "../lib/tauri";
import type { WatcherStatus } from "../lib/protocol";

const OFF: WatcherStatus = { state: "off", message: null };

interface WatcherStore {
  status: WatcherStatus;
  set: (status: WatcherStatus) => void;
  /** Pulls the current status once (used on mount/reload, before any event arrives). */
  refresh: () => Promise<void>;
  reset: () => void;
}

export const useWatcherStore = create<WatcherStore>((set) => ({
  status: OFF,

  set: (status) => set({ status }),

  refresh: async () => {
    try {
      const status = await watcherStatus();
      set({ status });
    } catch {
      set({ status: OFF });
    }
  },

  reset: () => set({ status: OFF }),
}));
