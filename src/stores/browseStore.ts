import { create } from "zustand";
import { browseDir, connect } from "../lib/tauri";
import type { BrowseListing, ConnectionTarget } from "../lib/protocol";
import { useConnectionStore } from "./connectionStore";

function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Whether two targets name the same machine. */
function sameTarget(a: ConnectionTarget, b: ConnectionTarget): boolean {
  if (a.kind === "wsl" && b.kind === "wsl") return a.distro === b.distro;
  if (a.kind === "ssh" && b.kind === "ssh") return a.host === b.host;
  return a.kind === "local" && b.kind === "local";
}

/**
 * The folder picker for whichever machine is connected.
 *
 * Separate from `treeStore` for the same reason `core::browse` is separate
 * from `core::tree`: this walks absolute paths on a machine with no workspace
 * open, applies no ignore rules, and exists precisely to answer the question
 * the tree assumes has already been answered.
 */
interface BrowseStore {
  /** `null` until the first listing lands. */
  listing: BrowseListing | null;
  /** Where the picker is pointed, for the location it builds on open. */
  target: ConnectionTarget | null;
  loading: boolean;
  error: string | null;
  /** Connect to `target` if it isn't already, then list its home directory. */
  start: (target: ConnectionTarget) => Promise<void>;
  /** List `path`, or the home directory when null. */
  go: (path: string | null) => Promise<void>;
  close: () => void;
}

export const useBrowseStore = create<BrowseStore>((set, get) => ({
  listing: null,
  target: null,
  loading: false,
  error: null,

  start: async (target) => {
    set({ loading: true, error: null, listing: null, target });

    // Reconnecting a link that is already up would tear down a working daemon
    // and, over SSH, can mean a second passphrase prompt — for a listing the
    // existing connection can already answer.
    const live = useConnectionStore.getState().info;
    if (live.state === "connected" && sameTarget(live.target, target)) {
      await get().go(null);
      return;
    }

    try {
      // Connecting owns the slow, interesting part — it may install a daemon,
      // prompt for a passphrase, or fail. The listing after it is cheap.
      useConnectionStore.getState().apply(await connect(target));
    } catch (err) {
      // `target` deliberately stays set: the browser panel is the only place
      // this error is shown, and clearing it would unmount the panel and take
      // the explanation with it.
      set({ loading: false, error: toErrorMessage(err) });
      await useConnectionStore.getState().refresh();
      return;
    }
    await get().go(null);
  },

  go: async (path) => {
    set({ loading: true, error: null });
    try {
      set({ listing: await browseDir(path), loading: false });
    } catch (err) {
      // A directory that can't be read leaves the previous listing on screen:
      // clearing it would strand the user with no way back up.
      set({ loading: false, error: toErrorMessage(err) });
    }
  },

  close: () => set({ listing: null, target: null, loading: false, error: null }),
}));
