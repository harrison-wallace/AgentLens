import { create } from "zustand";
import { listFiles } from "../lib/tauri";

interface PaletteStore {
  open: boolean;
  /** Flat file index, fetched once per open so it reflects recent changes. */
  files: string[];
  loading: boolean;
  query: string;
  /** Index into the currently filtered results, not into `files`. */
  cursor: number;
  show: () => Promise<void>;
  hide: () => void;
  setQuery: (query: string) => void;
  moveCursor: (delta: number, resultCount: number) => void;
  reset: () => void;
}

export const usePaletteStore = create<PaletteStore>((set, get) => ({
  open: false,
  files: [],
  loading: false,
  query: "",
  cursor: 0,

  show: async () => {
    set({ open: true, query: "", cursor: 0, loading: true });
    try {
      set({ files: await listFiles(), loading: false });
    } catch {
      set({ files: [], loading: false });
    }
  },

  hide: () => set({ open: false }),

  // Typing invalidates the previous selection, so the cursor goes back to the
  // best match rather than to whatever happened to be at that index.
  setQuery: (query) => set({ query, cursor: 0 }),

  moveCursor: (delta, resultCount) => {
    if (resultCount === 0) return;
    const next = (get().cursor + delta + resultCount) % resultCount;
    set({ cursor: next });
  },

  reset: () => set({ open: false, files: [], loading: false, query: "", cursor: 0 }),
}));
