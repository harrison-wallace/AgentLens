import { create } from "zustand";
import { connection, disconnect, wslDistros } from "../lib/tauri";
import type { ConnectionInfo } from "../lib/protocol";

/** Until the first read lands. The app always starts observing this machine. */
const LOCAL: ConnectionInfo = {
  target: { kind: "local" },
  state: "connected",
  label: "Local",
  remote: false,
  message: null,
  daemonVersion: null,
  since: 0,
};

/**
 * Which machine is being observed.
 *
 * Deliberately has no "connect" action: opening a workspace is what changes
 * machines, because a location (`ssh://box/srv/app`) already says which one it
 * is on and connecting without something to open would leave the app pointed
 * at a machine with nothing to show. The only move in the other direction —
 * back to this machine — is `goLocal`.
 */
interface ConnectionStore {
  info: ConnectionInfo;
  /** WSL distros offered in the picker; empty off Windows. */
  distros: string[];
  refresh: () => Promise<void>;
  loadDistros: () => Promise<void>;
  goLocal: () => Promise<void>;
  /** Applied from the `connection` event, which is the only source after connect. */
  apply: (info: ConnectionInfo) => void;
}

export const useConnectionStore = create<ConnectionStore>((set) => ({
  info: LOCAL,
  distros: [],

  refresh: async () => {
    try {
      set({ info: await connection() });
    } catch {
      // No backend to ask (a plain browser, say) — local is the honest
      // assumption rather than a scary banner.
      set({ info: LOCAL });
    }
  },

  loadDistros: async () => {
    try {
      set({ distros: await wslDistros() });
    } catch {
      set({ distros: [] });
    }
  },

  goLocal: async () => {
    try {
      set({ info: await disconnect() });
    } catch {
      // Going local is the fallback, so there is nowhere sensible to fall
      // back to. Re-read instead of guessing, and let the status bar show
      // whatever is actually true.
      await useConnectionStore.getState().refresh();
    }
  },

  apply: (info) => set({ info }),
}));
