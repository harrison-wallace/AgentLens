import { create } from "zustand";
import { setWorkspaceSettings, workspaceSettings } from "../lib/tauri";
import type { WorkspaceSettings } from "../lib/protocol";

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

const EMPTY: WorkspaceSettings = { extraIgnores: [] };

interface SettingsStore {
  settings: WorkspaceSettings;
  open: boolean;
  saving: boolean;
  error: string | null;
  setOpen: (open: boolean) => void;
  refresh: () => Promise<void>;
  /** Persists the globs and returns true if the save succeeded. */
  save: (extraIgnores: string[]) => Promise<boolean>;
  reset: () => void;
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  settings: EMPTY,
  open: false,
  saving: false,
  error: null,

  setOpen: (open) => set({ open, error: null }),

  refresh: async () => {
    try {
      set({ settings: await workspaceSettings(), error: null });
    } catch {
      // No workspace open — defaults are the right answer, not an error.
      set({ settings: EMPTY });
    }
  },

  save: async (extraIgnores) => {
    set({ saving: true, error: null });
    try {
      set({ settings: await setWorkspaceSettings({ extraIgnores }), saving: false });
      return true;
    } catch (err) {
      set({ saving: false, error: toErrorMessage(err) });
      return false;
    }
  },

  reset: () => set({ settings: EMPTY, open: false, saving: false, error: null }),
}));
