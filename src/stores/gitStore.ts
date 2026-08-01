import { create } from "zustand";
import { gitStatus } from "../lib/tauri";
import type { GitStatusKind, GitStatusSnapshot } from "../lib/protocol";

function toStatusByPath(status: GitStatusSnapshot): Record<string, GitStatusKind> {
  const statusByPath: Record<string, GitStatusKind> = {};
  for (const file of status.files) {
    statusByPath[file.path] = file.status;
  }
  return statusByPath;
}

interface GitStore {
  status: GitStatusSnapshot | null;
  /** Derived once per refresh so tree rows do O(1) lookups. */
  statusByPath: Record<string, GitStatusKind>;
  refresh: () => Promise<void>;
  /** Applies a snapshot already pushed by the `git-status` event, skipping
   * the round-trip `refresh()` would make. */
  applySnapshot: (status: GitStatusSnapshot) => void;
  reset: () => void;
}

export const useGitStore = create<GitStore>((set) => ({
  status: null,
  statusByPath: {},

  refresh: async () => {
    try {
      const status = await gitStatus();
      set({ status, statusByPath: toStatusByPath(status) });
    } catch {
      // No workspace open, or the read failed — fall back to "unknown"
      // rather than throwing into the render tree.
      set({ status: null, statusByPath: {} });
    }
  },

  applySnapshot: (status) => set({ status, statusByPath: toStatusByPath(status) }),

  reset: () => set({ status: null, statusByPath: {} }),
}));
