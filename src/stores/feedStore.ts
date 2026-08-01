import { create } from "zustand";
import type { FsEvent } from "../lib/protocol";

/**
 * The feed renders every entry it holds, so this bound is a DOM-node budget
 * as much as a memory one: at up to 20 rendered rows per entry, 100 keeps the
 * worst case near 2k nodes. It's a live activity view, not the history — that
 * arrives with session snapshots.
 */
const MAX_ENTRIES = 100;

/** One debounced batch, grouped under a single timestamp for the feed. */
export interface FeedEntry {
  id: string;
  at: number;
  events: FsEvent[];
}

interface FeedStore {
  /** Newest-first, bounded at `MAX_ENTRIES`. */
  entries: FeedEntry[];
  addBatch: (events: FsEvent[]) => void;
  clear: () => void;
}

let nextId = 0;

export const useFeedStore = create<FeedStore>((set, get) => ({
  entries: [],

  addBatch: (events) => {
    if (events.length === 0) return;
    const entry: FeedEntry = { id: `${Date.now()}-${nextId++}`, at: Date.now(), events };
    set({ entries: [entry, ...get().entries].slice(0, MAX_ENTRIES) });
  },

  clear: () => set({ entries: [] }),
}));
