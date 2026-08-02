import { create } from "zustand";
import {
  agentRoots,
  appSettings,
  pinnedEntries,
  setAppSettings,
  setWorkspaceSettings,
  workspaceSettings,
} from "../lib/tauri";
import type { AgentRootInfo, AppSettings, PinnedEntry, WorkspaceSettings } from "../lib/protocol";
import { useTreeStore } from "./treeStore";

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

const EMPTY: WorkspaceSettings = { extraIgnores: [], showIgnored: false, pinned: [] };
/** Only used before the first read lands; the backend owns the real default. */
const EMPTY_APP: AppSettings = {
  showAgentContext: true,
  agentRoots: [],
  daemonCommand: "agentlens-daemon",
};

interface SettingsStore {
  /** Scoped to the open workspace. */
  settings: WorkspaceSettings;
  /** Scoped to the app, and so outliving any one workspace. */
  app: AppSettings;
  /** The pinned paths resolved against the disk, for the tree's Pinned group. */
  pins: PinnedEntry[];
  /** Directories being searched for agent sessions, for the settings panel. */
  roots: AgentRootInfo[];
  open: boolean;
  saving: boolean;
  error: string | null;
  setOpen: (open: boolean) => void;
  refresh: () => Promise<void>;
  refreshApp: () => Promise<void>;
  refreshPins: () => Promise<void>;
  refreshRoots: () => Promise<void>;
  /** Persists the globs and returns true if the save succeeded. */
  save: (extraIgnores: string[]) => Promise<boolean>;
  /** Flips git-ignored visibility and persists it. */
  toggleShowIgnored: () => Promise<boolean>;
  /** Flips whether `AGENTS.md` and friends are surfaced, for every workspace. */
  toggleShowAgentContext: () => Promise<boolean>;
  /** Replaces the extra directories searched for agent sessions. */
  setAgentRoots: (roots: string[]) => Promise<boolean>;
  /** Replaces what is run on the far side of a WSL or SSH connection. */
  setDaemonCommand: (command: string) => Promise<boolean>;
  /** Pins `path` if it isn't already, unpins it if it is. */
  togglePin: (path: string) => Promise<boolean>;
  isPinned: (path: string) => boolean;
  /** Replaces the whole pinned list, for the bulk edit in settings. */
  setPinned: (pinned: string[]) => Promise<boolean>;
  reset: () => void;
}

type Setter = (patch: Partial<SettingsStore>) => void;

/**
 * Tail of the write chain. Saves run one at a time, so a write always builds
 * its payload from what the previous one actually stored — committing a
 * textarea on blur and clicking a toggle in the same gesture would otherwise
 * have the second overwrite the first from pre-save state.
 */
let pending: Promise<void> = Promise.resolve();

function enqueue<T>(task: () => Promise<T>): Promise<T> {
  // Both arms run `task`: one write failing must not stall every later one.
  const run = pending.then(task, task);
  pending = run.then(
    () => undefined,
    () => undefined,
  );
  return run;
}

/**
 * Single write path for both scopes. `write` is invoked once its turn comes
 * up, not when it is queued, so anything it reads out of the store is current.
 * Every setting here changes what the tree may show, so a successful save
 * re-reads everything already loaded — the backend has already restarted the
 * watcher.
 */
async function persist(
  set: Setter,
  write: () => Promise<Partial<SettingsStore>>,
): Promise<boolean> {
  return enqueue(async () => {
    set({ saving: true, error: null });
    try {
      set({ ...(await write()), saving: false });
    } catch (err) {
      set({ saving: false, error: toErrorMessage(err) });
      return false;
    }
    await useSettingsStore.getState().refreshPins();
    await useTreeStore.getState().reloadLoaded();
    return true;
  });
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
  settings: EMPTY,
  app: EMPTY_APP,
  pins: [],
  roots: [],
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
    await get().refreshPins();
  },

  refreshApp: async () => {
    try {
      set({ app: await appSettings() });
    } catch {
      set({ app: EMPTY_APP });
    }
    await get().refreshRoots();
  },

  refreshPins: async () => {
    try {
      set({ pins: await pinnedEntries() });
    } catch {
      set({ pins: [] });
    }
  },

  refreshRoots: async () => {
    try {
      set({ roots: await agentRoots() });
    } catch {
      set({ roots: [] });
    }
  },

  save: async (extraIgnores) =>
    persist(set, async () => ({
      settings: await setWorkspaceSettings({ ...get().settings, extraIgnores }),
    })),

  toggleShowIgnored: async () =>
    persist(set, async () => ({
      settings: await setWorkspaceSettings({
        ...get().settings,
        showIgnored: !get().settings.showIgnored,
      }),
    })),

  toggleShowAgentContext: async () =>
    persist(set, async () => ({
      app: await setAppSettings({
        ...get().app,
        showAgentContext: !get().app.showAgentContext,
      }),
    })),

  setAgentRoots: async (roots) =>
    persist(set, async () => ({
      app: await setAppSettings({ ...get().app, agentRoots: roots }),
    })),

  // Takes effect on the *next* connection: a daemon already running was
  // started by the old command and re-spawning it under the user's cursor
  // would drop their session to fix a setting they may be mid-typing.
  setDaemonCommand: async (command) =>
    persist(set, async () => ({
      app: await setAppSettings({
        ...get().app,
        daemonCommand: command.trim() || EMPTY_APP.daemonCommand,
      }),
    })),

  isPinned: (path) => get().settings.pinned.includes(path),

  // Which way this toggles is decided when the write runs, not when it is
  // requested, so two quick pins can't both read the same starting list.
  togglePin: async (path) =>
    persist(set, async () => {
      const current = get().settings;
      const pinned = current.pinned.includes(path)
        ? current.pinned.filter((entry) => entry !== path)
        : [...current.pinned, path];
      return { settings: await setWorkspaceSettings({ ...current, pinned }) };
    }),

  setPinned: async (pinned) =>
    persist(set, async () => ({
      settings: await setWorkspaceSettings({ ...get().settings, pinned }),
    })),

  // App-level settings survive: they aren't the workspace's to reset.
  reset: () => set({ settings: EMPTY, pins: [], open: false, saving: false, error: null }),
}));
