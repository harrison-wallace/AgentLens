import { create } from "zustand";
import { open } from "@tauri-apps/plugin-dialog";
import { closeWorkspace, currentWorkspace, openWorkspace, recentWorkspaces } from "../lib/tauri";
import type { WorkspaceInfo } from "../lib/protocol";

interface WorkspaceStore {
  workspace: WorkspaceInfo | null;
  recent: string[];
  error: string | null;
  open: (path: string) => Promise<void>;
  openViaDialog: () => Promise<void>;
  close: () => Promise<void>;
  restore: () => Promise<void>;
  loadRecent: () => Promise<void>;
}

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export const useWorkspaceStore = create<WorkspaceStore>((set, get) => ({
  workspace: null,
  recent: [],
  error: null,

  open: async (path) => {
    try {
      const workspace = await openWorkspace(path);
      set({ workspace, error: null });
    } catch (err) {
      set({ error: toErrorMessage(err) });
    }
  },

  openViaDialog: async () => {
    let selected: string | null;
    try {
      selected = await open({ directory: true, multiple: false });
    } catch (err) {
      set({ error: toErrorMessage(err) });
      return;
    }
    if (!selected) return;
    await get().open(selected);
  },

  close: async () => {
    try {
      await closeWorkspace();
      set({ workspace: null, error: null });
    } catch (err) {
      set({ error: toErrorMessage(err) });
    }
  },

  restore: async () => {
    try {
      const workspace = await currentWorkspace();
      set({ workspace });
    } catch (err) {
      set({ error: toErrorMessage(err) });
    }
  },

  loadRecent: async () => {
    try {
      const recent = await recentWorkspaces();
      set({ recent });
    } catch {
      // Recent list is a convenience, not critical — leave it empty.
      set({ recent: [] });
    }
  },
}));
