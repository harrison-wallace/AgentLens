import { create } from "zustand";
import { listFiles } from "../lib/tauri";

export type PaletteKind = "files" | "commands";

interface PaletteStore {
  open: boolean;
  kind: PaletteKind;
  /** Flat file index, fetched once per open so it reflects recent changes. */
  files: string[];
  loading: boolean;
  query: string;
  show: () => Promise<void>;
  showCommands: () => void;
  hide: () => void;
  setQuery: (query: string) => void;
  reset: () => void;
}

export const usePaletteStore = create<PaletteStore>((set) => ({
  open: false,
  kind: "files",
  files: [],
  loading: false,
  query: "",

  show: async () => {
    set({ open: true, kind: "files", query: "", loading: true });
    try {
      set({ files: await listFiles(), loading: false });
    } catch {
      set({ files: [], loading: false });
    }
  },

  showCommands: () => set({ open: true, kind: "commands", query: "", loading: false }),

  hide: () => set({ open: false }),

  setQuery: (query) => set({ query }),

  reset: () => set({ open: false, kind: "files", files: [], loading: false, query: "" }),
}));
