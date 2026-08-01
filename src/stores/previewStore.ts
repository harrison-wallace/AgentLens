import { create } from "zustand";
import { readPreview, sessionDiff } from "../lib/tauri";
import type { PreviewPayload, SessionDiff } from "../lib/protocol";

export type PreviewTab = "current" | "diff";

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

interface PreviewStore {
  path: string | null;
  tab: PreviewTab;
  payload: PreviewPayload | null;
  diff: SessionDiff | null;
  loading: boolean;
  error: string | null;
  /** Loads whichever side the active tab needs; safe to call repeatedly. */
  load: (path: string) => Promise<void>;
  setTab: (tab: PreviewTab) => Promise<void>;
  /** Re-reads the open file, e.g. after the watcher reports it changed. */
  refresh: () => Promise<void>;
  reset: () => void;
}

const EMPTY = {
  path: null,
  tab: "current" as PreviewTab,
  payload: null,
  diff: null,
  loading: false,
  error: null,
};

export const usePreviewStore = create<PreviewStore>((set, get) => ({
  ...EMPTY,

  load: async (path) => {
    const tab = get().tab;
    // Switching files clears the previous payload immediately so a slow read
    // can't leave the last file's content on screen under the new name.
    set({ path, payload: null, diff: null, loading: true, error: null });

    // Arrowing through the tree fires a load per row, so reads land out of
    // order. Anything that resolves after the selection moved on is dropped
    // rather than painted over the file the user is actually looking at.
    const isStale = () => get().path !== path || get().tab !== tab;

    try {
      if (tab === "diff") {
        const diff = await sessionDiff(path);
        if (!isStale()) set({ diff, loading: false });
      } else {
        const payload = await readPreview(path);
        if (!isStale()) set({ payload, loading: false });
      }
    } catch (err) {
      if (!isStale()) set({ loading: false, error: toErrorMessage(err) });
    }
  },

  setTab: async (tab) => {
    set({ tab });
    const path = get().path;
    if (path) await get().load(path);
  },

  refresh: async () => {
    const path = get().path;
    if (path) await get().load(path);
  },

  reset: () => set({ ...EMPTY }),
}));
