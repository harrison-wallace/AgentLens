import { create } from "zustand";
import { setWorkspaceSettings, workspaceSettings } from "../lib/tauri";
import type { WorkspaceSettings } from "../lib/protocol";

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

const EMPTY: WorkspaceSettings = { extraIgnores: [], showIgnored: false };

interface SettingsStore {
  settings: WorkspaceSettings;
  open: boolean;
  saving: boolean;
  error: string | null;
  setOpen: (open: boolean) => void;
  refresh: () => Promise<void>;
  /** Persists the globs and returns true if the save succeeded. */
  save: (extraIgnores: string[]) => Promise<boolean>;
  /** Flips git-ignored visibility and persists it. */
  toggleShowIgnored: () => Promise<boolean>;
  reset: () => void;
}

type Setter = (patch: Partial<SettingsStore>) => void;

/** Single write path, so both callers stay in step on saving/error state. */
async function persist(set: Setter, next: WorkspaceSettings): Promise<boolean> {
  set({ saving: true, error: null });
  try {
    set({ settings: await setWorkspaceSettings(next), saving: false });
    return true;
  } catch (err) {
    set({ saving: false, error: toErrorMessage(err) });
    return false;
  }
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
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
    return persist(set, { ...get().settings, extraIgnores });
  },

  toggleShowIgnored: async () => {
    const settings = get().settings;
    return persist(set, { ...settings, showIgnored: !settings.showIgnored });
  },

  reset: () => set({ settings: EMPTY, open: false, saving: false, error: null }),
}));
