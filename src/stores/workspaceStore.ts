import { create } from "zustand";
import { open } from "@tauri-apps/plugin-dialog";
import {
  closeWorkspace,
  currentWorkspace,
  openWorkspace,
  recentWorkspaces,
  restartSession,
} from "../lib/tauri";
import type { WorkspaceInfo } from "../lib/protocol";
import { useConnectionStore } from "./connectionStore";
import { useToastStore } from "./toastStore";

interface WorkspaceStore {
  workspace: WorkspaceInfo | null;
  recent: string[];
  error: string | null;
  /**
   * True while an open is in flight. Only worth surfacing for a remote one,
   * where connecting can mean waiting on SSH auth — but the flag is set for
   * every open so there is one code path.
   */
  opening: boolean;
  /**
   * Open a workspace. `path` may name another machine
   * (`wsl://Ubuntu/home/h/proj`, `ssh://box/srv/app`); the backend connects
   * there first.
   */
  open: (path: string) => Promise<void>;
  openViaDialog: () => Promise<void>;
  close: () => Promise<void>;
  /** Resets "since when" for highlights and diffs, keeping the workspace. */
  restartSession: () => Promise<void>;
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
  opening: false,

  open: async (path) => {
    // Empty-state already renders `error` inline; toast only when that
    // screen is not the one the user is looking at.
    const hadWorkspace = get().workspace !== null;
    set({ opening: true, error: null });
    try {
      const workspace = await openWorkspace(path);
      set({ workspace, error: null, opening: false });
      // A remote open changes which machine is being observed, and a failed
      // one can leave the connection somewhere new too.
      await useConnectionStore.getState().refresh();
      await get().loadRecent();
    } catch (err) {
      const message = toErrorMessage(err);
      set({ error: message, opening: false });
      if (hadWorkspace) {
        useToastStore.getState().push("Couldn't open workspace", message);
      }
      await useConnectionStore.getState().refresh();
      // Opening elsewhere replaces the backend, which closes whatever the
      // previous one had open — so a failure part-way through can leave this
      // store describing a workspace that no longer exists on any machine.
      // Believe the backend rather than the last thing that worked.
      await get().restore();
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

  restartSession: async () => {
    try {
      const workspace = await restartSession();
      set({ workspace, error: null });
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
