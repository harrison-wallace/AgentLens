import { create } from "zustand";
import { listDir } from "../lib/tauri";
import { parentDirsOf } from "../lib/treeRows";
import type { DirEntryNode, FsEvent } from "../lib/protocol";

/** How long a changed row keeps its glow. */
const GLOW_DURATION_MS = 60_000;

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
  /** True when `selected` is a directory, so the preview knows to stay put. */
  selectedIsDir: boolean;
  /** Path -> epoch ms it last changed; drives the tree-row glow. */
  recentlyChanged: Record<string, number>;
  loadDir: (path: string) => Promise<void>;
  toggle: (path: string) => void;
  /** Collapses a directory without the toggle's expand-and-load branch. */
  collapse: (path: string) => void;
  select: (path: string, isDir: boolean) => void;
  /** Expands every ancestor directory of `path` (loading as needed) and
   * selects it — used by the activity feed's "reveal in tree" row click. */
  revealPath: (path: string) => Promise<void>;
  /** Re-fetches every directory already loaded (used by the refresh button). */
  reloadLoaded: () => Promise<void>;
  /**
   * Records glow timestamps for the changed paths, then reloads only the
   * distinct parent directories that are currently loaded — simpler and
   * more correct than surgical node insertion, and the watcher's debounce
   * keeps it cheap.
   */
  applyFsChanges: (events: FsEvent[]) => void;
  /** Drops glow entries older than `GLOW_DURATION_MS`. */
  pruneGlow: (now: number) => void;
  /** Drops every glow at once, when the session is re-baselined. */
  clearGlow: () => void;
  reset: () => void;
}

export const useTreeStore = create<TreeStore>((set, get) => ({
  childrenByPath: {},
  expanded: new Set(),
  loading: new Set(),
  errors: {},
  selected: null,
  selectedIsDir: false,
  recentlyChanged: {},

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

  collapse: (path) => {
    const expanded = new Set(get().expanded);
    if (expanded.delete(path)) set({ expanded });
  },

  select: (path, isDir) => set({ selected: path, selectedIsDir: isDir }),

  revealPath: async (path) => {
    const segments = path.split("/");
    segments.pop(); // drop the file/dir name itself, keep only ancestors
    let ancestor = "";
    for (const segment of segments) {
      ancestor = ancestor ? `${ancestor}/${segment}` : segment;
      if (!get().expanded.has(ancestor)) {
        set({ expanded: new Set(get().expanded).add(ancestor) });
      }
      if (!get().childrenByPath[ancestor]) {
        await get().loadDir(ancestor);
      }
    }
    set({ selected: path, selectedIsDir: false });
  },

  reloadLoaded: async () => {
    const paths = Object.keys(get().childrenByPath);
    await Promise.all(paths.map((path) => get().loadDir(path)));
  },

  applyFsChanges: (events) => {
    if (events.length === 0) return;

    const now = Date.now();
    const recentlyChanged = { ...get().recentlyChanged };
    for (const event of events) {
      recentlyChanged[event.path] = now;
    }
    set({ recentlyChanged });

    const childrenByPath = get().childrenByPath;
    const dirs = parentDirsOf(events.map((e) => e.path));
    for (const dir of dirs) {
      if (dir in childrenByPath) {
        void get().loadDir(dir);
      }
    }
  },

  pruneGlow: (now) => {
    const recentlyChanged = get().recentlyChanged;
    const next: Record<string, number> = {};
    let changed = false;
    for (const [path, at] of Object.entries(recentlyChanged)) {
      if (now - at < GLOW_DURATION_MS) {
        next[path] = at;
      } else {
        changed = true;
      }
    }
    if (changed) set({ recentlyChanged: next });
  },

  clearGlow: () => set({ recentlyChanged: {} }),

  reset: () =>
    set({
      childrenByPath: {},
      expanded: new Set(),
      loading: new Set(),
      errors: {},
      selected: null,
      selectedIsDir: false,
      recentlyChanged: {},
    }),
}));
