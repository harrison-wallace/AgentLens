import { create } from "zustand";
import {
  gitBranches,
  gitCapabilities,
  gitCommit,
  gitStage,
  gitStageAll,
  gitStashPop,
  gitStashPush,
  gitStatus,
  gitSwitchBranch,
  gitUnstage,
  gitUnstageAll,
  gitCreateBranch,
} from "../lib/tauri";
import type {
  BranchList,
  GitCapabilities,
  GitStatusKind,
  GitStatusSnapshot,
} from "../lib/protocol";

/**
 * One badge kind per path, for the tree.
 *
 * A path can now appear twice — staged work and unstaged work are separate
 * entries — so this picks the working-tree side when both exist. That is what
 * the row is showing: how the file on disk differs, which is the question a
 * file tree answers.
 */
function toStatusByPath(status: GitStatusSnapshot): Record<string, GitStatusKind> {
  const statusByPath: Record<string, GitStatusKind> = {};
  for (const file of status.files) {
    if (!file.staged || !(file.path in statusByPath)) {
      statusByPath[file.path] = file.status;
    }
  }
  return statusByPath;
}

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

interface GitStore {
  status: GitStatusSnapshot | null;
  /** Derived once per refresh so tree rows do O(1) lookups. */
  statusByPath: Record<string, GitStatusKind>;
  /**
   * Whether mutations can be offered at all; read once per workspace.
   * `null` means "not read yet" — distinct from "git is missing", so the
   * panel doesn't flash an unavailable notice while it's still loading.
   */
  capabilities: GitCapabilities | null;
  branches: BranchList | null;
  /** True while a mutation is in flight — hooks can take seconds. */
  busy: boolean;
  /**
   * Last mutation failure, verbatim from git. Cleared on the next attempt.
   * Kept as text rather than shown in a dialog: git's own message is what
   * tells the user what to do, and a modal would interrupt to say it.
   */
  error: string | null;
  /**
   * The branch a switch just failed to reach, so the failure can offer the
   * fix instead of only describing it. Cleared by the next mutation.
   */
  failedSwitch: string | null;
  refresh: () => Promise<void>;
  refreshCapabilities: () => Promise<void>;
  refreshBranches: () => Promise<void>;
  /** Applies a snapshot already pushed by the `git-status` event, skipping
   * the round-trip `refresh()` would make. */
  applySnapshot: (status: GitStatusSnapshot) => void;
  stage: (paths: string[]) => Promise<boolean>;
  unstage: (paths: string[]) => Promise<boolean>;
  stageAll: () => Promise<boolean>;
  unstageAll: () => Promise<boolean>;
  commit: (message: string, amend?: boolean) => Promise<boolean>;
  switchBranch: (name: string) => Promise<boolean>;
  /** Stash, switch, and put the work back if the switch still fails. */
  stashAndSwitch: (name: string) => Promise<boolean>;
  createBranch: (name: string) => Promise<boolean>;
  stashPush: (message?: string) => Promise<boolean>;
  stashPop: () => Promise<boolean>;
  dismissError: () => void;
  reset: () => void;
}

type Setter = (patch: Partial<GitStore>) => void;

/**
 * Single write path. Every mutation returns the refreshed status, so the
 * panel updates from the same response that performed the change rather than
 * racing a separate read.
 *
 * Mutations are serialized: a hook can take seconds, and two overlapping
 * `git` processes in one repository can collide on the index lock.
 */
let pending: Promise<void> = Promise.resolve();

async function mutate(set: Setter, op: () => Promise<GitStatusSnapshot>): Promise<boolean> {
  const run = pending.then(
    () => attempt(set, op),
    () => attempt(set, op),
  );
  pending = run.then(
    () => undefined,
    () => undefined,
  );
  return run;
}

async function attempt(set: Setter, op: () => Promise<GitStatusSnapshot>): Promise<boolean> {
  set({ busy: true, error: null, failedSwitch: null });
  try {
    const status = await op();
    set({ status, statusByPath: toStatusByPath(status), busy: false });
    return true;
  } catch (err) {
    set({ busy: false, error: toErrorMessage(err) });
    return false;
  }
}

export const useGitStore = create<GitStore>((set, get) => ({
  status: null,
  statusByPath: {},
  capabilities: null,
  branches: null,
  busy: false,
  error: null,
  failedSwitch: null,

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

  refreshCapabilities: async () => {
    try {
      set({ capabilities: await gitCapabilities() });
    } catch (err) {
      set({ capabilities: { canMutate: false, version: null, reason: toErrorMessage(err) } });
    }
  },

  refreshBranches: async () => {
    try {
      set({ branches: await gitBranches() });
    } catch {
      set({ branches: null });
    }
  },

  applySnapshot: (status) => set({ status, statusByPath: toStatusByPath(status) }),

  stage: (paths) => mutate(set, () => gitStage(paths)),
  unstage: (paths) => mutate(set, () => gitUnstage(paths)),
  stageAll: () => mutate(set, () => gitStageAll()),
  unstageAll: () => mutate(set, () => gitUnstageAll()),
  commit: (message, amend = false) => mutate(set, () => gitCommit(message, amend)),

  // Branch and stash operations change which branch is checked out, so the
  // list has to be re-read afterwards, not just the file status.
  switchBranch: async (name) => {
    const ok = await mutate(set, () => gitSwitchBranch(name));
    if (ok) await get().refreshBranches();
    // Remembered so the error can offer to stash; `attempt` clears it as soon
    // as anything else is attempted.
    else set({ failedSwitch: name });
    return ok;
  },

  // The four-step dance the error used to leave to the user: stash, switch,
  // and — if the switch fails for a reason stashing was never going to fix —
  // put the work straight back rather than leaving it parked in a stash they
  // did not ask for.
  stashAndSwitch: async (name) => {
    if (!(await get().stashPush())) return false;
    if (await get().switchBranch(name)) return true;
    // Popping is itself a mutation, and a successful one clears the error —
    // including the switch failure that is the whole reason for popping. Put
    // it back only when the pop itself succeeded; a failed pop's own message
    // (conflict, empty stash) is the one the user needs to act on.
    const failure = get().error;
    if (await get().stashPop()) set({ error: failure });
    return false;
  },

  createBranch: async (name) => {
    const ok = await mutate(set, () => gitCreateBranch(name));
    if (ok) await get().refreshBranches();
    return ok;
  },

  stashPush: (message) => mutate(set, () => gitStashPush(message)),
  stashPop: () => mutate(set, () => gitStashPop()),

  dismissError: () => set({ error: null, failedSwitch: null }),

  reset: () =>
    set({
      status: null,
      statusByPath: {},
      capabilities: null,
      branches: null,
      busy: false,
      error: null,
      failedSwitch: null,
    }),
}));
