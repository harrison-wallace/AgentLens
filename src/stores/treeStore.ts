import { create } from "zustand";
import { listDir } from "../lib/tauri";
import type { DirEntryNode } from "../lib/protocol";

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

interface TreeStore {
  /** Key `""` is the workspace root. */
  childrenByPath: Record<string, DirEntryNode[]>;
  expanded: Set<string>;
  loading: Set<string>;
  errors: Record<string, string>;
  selected: string | null;
  loadDir: (path: string) => Promise<void>;
  toggle: (path: string) => void;
  select: (path: string) => void;
  /** Re-fetches every directory already loaded (used by the refresh button). */
  reloadLoaded: () => Promise<void>;
  reset: () => void;
}

export const useTreeStore = create<TreeStore>((set, get) => ({
  childrenByPath: {},
  expanded: new Set(),
  loading: new Set(),
  errors: {},
  selected: null,

  loadDir: async (path) => {
    const loading = new Set(get().loading);
    loading.add(path);
    set({ loading });

    try {
      const children = await listDir(path);
      const errors = { ...get().errors };
      delete errors[path];
      const nextLoading = new Set(get().loading);
      nextLoading.delete(path);
      set({
        childrenByPath: { ...get().childrenByPath, [path]: children },
        errors,
        loading: nextLoading,
      });
    } catch (err) {
      const nextLoading = new Set(get().loading);
      nextLoading.delete(path);
      set({
        errors: { ...get().errors, [path]: toErrorMessage(err) },
        loading: nextLoading,
      });
    }
  },

  toggle: (path) => {
    const expanded = new Set(get().expanded);
    if (expanded.has(path)) {
      expanded.delete(path);
      set({ expanded });
      return;
    }
    expanded.add(path);
    set({ expanded });
    if (!get().childrenByPath[path]) {
      void get().loadDir(path);
    }
  },

  select: (path) => set({ selected: path }),

  reloadLoaded: async () => {
    const paths = Object.keys(get().childrenByPath);
    await Promise.all(paths.map((path) => get().loadDir(path)));
  },

  reset: () =>
    set({
      childrenByPath: {},
      expanded: new Set(),
      loading: new Set(),
      errors: {},
      selected: null,
    }),
}));
