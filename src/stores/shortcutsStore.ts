import { create } from "zustand";

interface ShortcutsStore {
  open: boolean;
  show: () => void;
  hide: () => void;
}

export const useShortcutsStore = create<ShortcutsStore>((set) => ({
  open: false,
  show: () => set({ open: true }),
  hide: () => set({ open: false }),
}));
