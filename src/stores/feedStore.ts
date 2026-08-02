import { create } from "zustand";
import type { FsEvent } from "../lib/protocol";

/**
 * The feed renders every entry it holds, so this bound is a DOM-node budget
 * as much as a memory one: at up to 20 rendered rows per entry, 100 keeps the
 * worst case near 2k nodes. It's a live activity view, not the history — that
 * arrives with session snapshots.
 */
const MAX_ENTRIES = 100;

/**
 * One thing in the feed: a debounced batch of changes, or a window in which
 * the app was not being told about any.
 *
 * The gap is not decoration. A remote connection *will* drop, and a feed that
 * simply resumes is claiming nothing happened during the outage — which is the
 * one thing it cannot know. Marking the window is the honest answer.
 */
export type FeedEntry =
  | { kind: "batch"; id: string; at: number; events: FsEvent[] }
  | {
      kind: "gap";
      id: string;
      at: number;
      from: number;
      /** `null` while the connection is still down. */
      to: number | null;
    };

interface FeedStore {
  /** Newest-first, bounded at `MAX_ENTRIES`. */
  entries: FeedEntry[];
  addBatch: (events: FsEvent[]) => void;
  /** The connection went down at `at`; start marking the window. */
  beginGap: (at: number) => void;
  /** The connection came back at `at`; close the open window, if any. */
  endGap: (at: number) => void;
  clear: () => void;
}

let nextId = 0;

function id(): string {
  return `${Date.now()}-${nextId++}`;
}

type Gap = Extract<FeedEntry, { kind: "gap" }>;

/** A window the app is still blind for — there is at most one. */
function isOpenGap(entry: FeedEntry): entry is Gap {
  return entry.kind === "gap" && entry.to === null;
}

export const useFeedStore = create<FeedStore>((set, get) => ({
  entries: [],

  addBatch: (events) => {
    if (events.length === 0) return;
    const entry: FeedEntry = { kind: "batch", id: id(), at: Date.now(), events };
    set({ entries: [entry, ...get().entries].slice(0, MAX_ENTRIES) });
  },

  beginGap: (at) => {
    const entries = get().entries;
    // Reconnect attempts can fail repeatedly; one outage is one gap, not one
    // per attempt.
    if (entries.some(isOpenGap)) return;
    const entry: FeedEntry = { kind: "gap", id: id(), at, from: at, to: null };
    set({ entries: [entry, ...entries].slice(0, MAX_ENTRIES) });
  },

  // Searched for rather than assumed to be newest: a reconnecting daemon
  // starts its watcher as part of being restored, so a batch of changes can
  // land ahead of the event announcing the connection is back. Requiring the
  // gap to be on top would leave it open forever, which is precisely the lie
  // the marker exists to prevent.
  endGap: (at) => {
    const entries = get().entries;
    if (!entries.some(isOpenGap)) return;
    set({
      entries: entries.map((entry) => (isOpenGap(entry) ? { ...entry, to: at } : entry)),
    });
  },

  clear: () => set({ entries: [] }),
}));
