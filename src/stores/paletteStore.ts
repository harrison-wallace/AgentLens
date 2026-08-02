import { create } from "zustand";
import { listFiles } from "../lib/tauri";

interface PaletteStore {
  open: boolean;
  /** Flat file index, fetched once per open so it reflects recent changes. */
  files: string[];
  loading: boolean;
  query: string;
  show: () => Promise<void>;
  hide: () => void;
  setQuery: (query: string) => void;
  reset: () => void;
}

export const usePaletteStore = create<PaletteStore>((set) => ({
  open: false,
  files: [],
  loading: false,
  query: "",

  show: async () => {
    set({ open: true, query: "", loading: true });
    try {
      set({ files: await listFiles(), loading: false });
    } catch {
      set({ files: [], loading: false });
    }
  },

  hide: () => set({ open: false }),

  setQuery: (query) => set({ query }),

  reset: () => set({ open: false, files: [], loading: false, query: "" }),
}));
