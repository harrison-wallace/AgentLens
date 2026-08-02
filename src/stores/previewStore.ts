import { create } from "zustand";
import { readPreview, sessionDiff } from "../lib/tauri";
import type { PreviewPayload, SessionDiff } from "../lib/protocol";

/** Current file body vs session diff — a mode of the active tab, not a tab. */
export type PreviewMode = "current" | "diff";

/** Soft cap; oldest permanent tabs drop when a new one would exceed it. */
export const MAX_OPEN_TABS = 20;

const TABS_STORAGE_KEY = "agentlens.open-tabs";

export interface OpenTab {
  path: string;
  /** False = VS Code-style preview tab (italic, replaced by the next preview). */
  permanent: boolean;
  mode: PreviewMode;
}

interface PathCache {
  payload: PreviewPayload | null;
  diff: SessionDiff | null;
  /** Which side(s) have been fetched at least once. */
  hasPayload: boolean;
  hasDiff: boolean;
}

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

interface PersistedTabs {
  tabs: OpenTab[];
  active: string | null;
}

type TabsByWorkspace = Record<string, PersistedTabs>;

function loadAllPersisted(): TabsByWorkspace {
  try {
    const raw = localStorage.getItem(TABS_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as TabsByWorkspace;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function persistWorkspace(workspaceKey: string, tabs: OpenTab[], active: string | null): void {
  try {
    const all = loadAllPersisted();
    if (tabs.length === 0) {
      delete all[workspaceKey];
    } else {
      all[workspaceKey] = { tabs, active };
    }
    localStorage.setItem(TABS_STORAGE_KEY, JSON.stringify(all));
  } catch {
    // Losing open tabs is not worth surfacing.
  }
}

function emptyContent() {
  return {
    payload: null as PreviewPayload | null,
    diff: null as SessionDiff | null,
    loading: false,
    error: null as string | null,
  };
}

interface PreviewStore {
  tabs: OpenTab[];
  activePath: string | null;
  /** Full location key (`formatLocation`) the open set belongs to. */
  workspaceKey: string | null;
  payload: PreviewPayload | null;
  diff: SessionDiff | null;
  loading: boolean;
  error: string | null;
  /**
   * VS Code single-click: reuse/replace the preview tab, or activate if
   * already open.
   */
  openPreview: (path: string) => Promise<void>;
  /** Double-click / palette / feed: keep a permanent tab. */
  openPermanent: (path: string) => Promise<void>;
  activate: (path: string) => Promise<void>;
  close: (path: string) => Promise<void>;
  closeActive: () => Promise<void>;
  nextTab: () => Promise<void>;
  prevTab: () => Promise<void>;
  setMode: (mode: PreviewMode) => Promise<void>;
  /** Re-read the active file (watcher hit). */
  refresh: () => Promise<void>;
  /** Invalidate cache for paths that changed on disk. */
  invalidate: (paths: Iterable<string>) => void;
  /** Bind to a workspace and restore its tabs; pass null to clear. */
  bindWorkspace: (workspaceKey: string | null) => Promise<void>;
  reset: () => void;
}

function tabIndex(tabs: OpenTab[], path: string): number {
  return tabs.findIndex((t) => t.path === path);
}

function withActive(tabs: OpenTab[], activePath: string | null): OpenTab | null {
  if (!activePath) return null;
  return tabs.find((t) => t.path === activePath) ?? null;
}

export const usePreviewStore = create<PreviewStore>((set, get) => {
  /** Per-path content cache so switching tabs does not re-fetch. */
  let cache = new Map<string, PathCache>();
  /** Generation counter so a slow read cannot paint over a newer selection. */
  let loadGen = 0;

  const remember = () => {
    const { workspaceKey, tabs, activePath } = get();
    if (workspaceKey) persistWorkspace(workspaceKey, tabs, activePath);
  };

  const paintFromCache = (path: string, mode: PreviewMode): boolean => {
    const entry = cache.get(path);
    if (!entry) return false;
    if (mode === "current" && entry.hasPayload) {
      set({
        payload: entry.payload,
        diff: null,
        loading: false,
        error: null,
      });
      return true;
    }
    if (mode === "diff" && entry.hasDiff) {
      set({
        payload: null,
        diff: entry.diff,
        loading: false,
        error: null,
      });
      return true;
    }
    return false;
  };

  const loadActive = async (path: string, mode: PreviewMode, force = false) => {
    const gen = ++loadGen;
    if (!force && paintFromCache(path, mode)) return;

    set({ loading: true, error: null, payload: null, diff: null });
    try {
      if (mode === "diff") {
        const diff = await sessionDiff(path);
        if (gen !== loadGen || get().activePath !== path) return;
        const prev = cache.get(path) ?? {
          payload: null,
          diff: null,
          hasPayload: false,
          hasDiff: false,
        };
        cache.set(path, { ...prev, diff, hasDiff: true });
        set({ diff, payload: null, loading: false, error: null });
      } else {
        const payload = await readPreview(path);
        if (gen !== loadGen || get().activePath !== path) return;
        const prev = cache.get(path) ?? {
          payload: null,
          diff: null,
          hasPayload: false,
          hasDiff: false,
        };
        cache.set(path, { ...prev, payload, hasPayload: true });
        set({ payload, diff: null, loading: false, error: null });
      }
    } catch (err) {
      if (gen !== loadGen || get().activePath !== path) return;
      set({ loading: false, error: toErrorMessage(err), payload: null, diff: null });
    }
  };

  const activatePath = async (path: string) => {
    const tab = withActive(get().tabs, path);
    if (!tab) return;
    set({ activePath: path, ...emptyContent() });
    remember();
    await loadActive(path, tab.mode);
  };

  const ensureTab = (path: string, permanent: boolean): OpenTab[] => {
    const tabs = [...get().tabs];
    const at = tabIndex(tabs, path);

    if (at !== -1) {
      const existing = tabs[at]!;
      if (permanent && !existing.permanent) {
        tabs[at] = { ...existing, permanent: true };
      }
      return tabs;
    }

    if (permanent) {
      // Cap open tabs: drop the oldest (leftmost) until there is room.
      while (tabs.length >= MAX_OPEN_TABS) {
        const removed = tabs.shift();
        if (removed) cache.delete(removed.path);
      }
      tabs.push({ path, permanent: true, mode: "current" });
      return tabs;
    }

    // Preview: replace existing non-permanent tab, or add one.
    const previewAt = tabs.findIndex((t) => !t.permanent);
    if (previewAt !== -1) {
      const old = tabs[previewAt]!;
      if (old.path !== path) cache.delete(old.path);
      tabs[previewAt] = { path, permanent: false, mode: "current" };
      return tabs;
    }

    if (tabs.length >= MAX_OPEN_TABS) {
      const removed = tabs.shift();
      if (removed) cache.delete(removed.path);
    }
    tabs.push({ path, permanent: false, mode: "current" });
    return tabs;
  };

  return {
    tabs: [],
    activePath: null,
    workspaceKey: null,
    ...emptyContent(),

    openPreview: async (path) => {
      const tabs = ensureTab(path, false);
      set({ tabs, activePath: path, ...emptyContent() });
      remember();
      const tab = tabs.find((t) => t.path === path)!;
      await loadActive(path, tab.mode);
    },

    openPermanent: async (path) => {
      const tabs = ensureTab(path, true);
      set({ tabs, activePath: path, ...emptyContent() });
      remember();
      const tab = tabs.find((t) => t.path === path)!;
      await loadActive(path, tab.mode);
    },

    activate: async (path) => {
      if (tabIndex(get().tabs, path) === -1) return;
      await activatePath(path);
    },

    close: async (path) => {
      const tabs = get().tabs;
      const at = tabIndex(tabs, path);
      if (at === -1) return;

      const next = tabs.filter((t) => t.path !== path);
      cache.delete(path);

      let activePath = get().activePath;
      if (activePath === path) {
        const neighbor = next[at] ?? next[at - 1] ?? null;
        activePath = neighbor?.path ?? null;
      }

      if (!activePath) {
        set({ tabs: next, activePath: null, ...emptyContent() });
        remember();
        return;
      }

      set({ tabs: next, activePath, ...emptyContent() });
      remember();
      const tab = next.find((t) => t.path === activePath);
      if (tab) await loadActive(activePath, tab.mode);
    },

    closeActive: async () => {
      const path = get().activePath;
      if (path) await get().close(path);
    },

    nextTab: async () => {
      const { tabs, activePath } = get();
      if (tabs.length < 2 || !activePath) return;
      const at = tabIndex(tabs, activePath);
      const next = tabs[(at + 1) % tabs.length];
      if (next) await activatePath(next.path);
    },

    prevTab: async () => {
      const { tabs, activePath } = get();
      if (tabs.length < 2 || !activePath) return;
      const at = tabIndex(tabs, activePath);
      const next = tabs[(at - 1 + tabs.length) % tabs.length];
      if (next) await activatePath(next.path);
    },

    setMode: async (mode) => {
      const { activePath, tabs } = get();
      if (!activePath) return;
      const at = tabIndex(tabs, activePath);
      if (at === -1) return;
      const current = tabs[at]!;
      if (current.mode === mode) return;
      const next = [...tabs];
      next[at] = { ...current, mode };
      set({ tabs: next, ...emptyContent() });
      remember();
      await loadActive(activePath, mode);
    },

    refresh: async () => {
      const { activePath, tabs } = get();
      if (!activePath) return;
      const tab = tabs.find((t) => t.path === activePath);
      if (!tab) return;
      cache.delete(activePath);
      await loadActive(activePath, tab.mode, true);
    },

    invalidate: (paths) => {
      for (const path of paths) cache.delete(path);
      const active = get().activePath;
      if (active && [...paths].includes(active)) {
        void get().refresh();
      }
    },

    bindWorkspace: async (workspaceKey) => {
      loadGen += 1;
      cache = new Map();
      if (!workspaceKey) {
        set({
          tabs: [],
          activePath: null,
          workspaceKey: null,
          ...emptyContent(),
        });
        return;
      }

      const saved = loadAllPersisted()[workspaceKey];
      const tabs = (saved?.tabs ?? [])
        .filter((t) => t && typeof t.path === "string" && t.path.length > 0)
        .slice(0, MAX_OPEN_TABS)
        .map((t) => ({
          path: t.path,
          permanent: Boolean(t.permanent),
          mode: t.mode === "diff" ? ("diff" as const) : ("current" as const),
        }));

      const activePath =
        saved?.active && tabs.some((t) => t.path === saved.active)
          ? saved.active
          : (tabs[0]?.path ?? null);

      set({ tabs, activePath, workspaceKey, ...emptyContent() });
      if (activePath) {
        const tab = tabs.find((t) => t.path === activePath)!;
        await loadActive(activePath, tab.mode);
      }
    },

    reset: () => {
      loadGen += 1;
      cache = new Map();
      set({
        tabs: [],
        activePath: null,
        workspaceKey: null,
        ...emptyContent(),
      });
    },
  };
});
