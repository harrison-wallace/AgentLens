import { create } from "zustand";

/** Whether the branch quick-pick is open — driven from the status bar or a command. */
interface BranchPickerStore {
  open: boolean;
  show: () => void;
  hide: () => void;
}

export const useBranchPickerStore = create<BranchPickerStore>((set) => ({
  open: false,
  show: () => set({ open: true }),
  hide: () => set({ open: false }),
}));
