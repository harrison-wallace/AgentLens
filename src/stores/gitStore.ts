import { create } from "zustand";
import { gitStatus } from "../lib/tauri";
import type { GitStatusKind, GitStatusSnapshot } from "../lib/protocol";

interface GitStore {
  status: GitStatusSnapshot | null;
  /** Derived once per refresh so tree rows do O(1) lookups. */
  statusByPath: Record<string, GitStatusKind>;
  refresh: () => Promise<void>;
  reset: () => void;
}

export const useGitStore = create<GitStore>((set) => ({
  status: null,
  statusByPath: {},

  refresh: async () => {
    try {
      const status = await gitStatus();
      const statusByPath: Record<string, GitStatusKind> = {};
      for (const file of status.files) {
        statusByPath[file.path] = file.status;
      }
      set({ status, statusByPath });
    } catch {
      // No workspace open, or the read failed — fall back to "unknown"
      // rather than throwing into the render tree.
      set({ status: null, statusByPath: {} });
    }
  },

  reset: () => set({ status: null, statusByPath: {} }),
}));
