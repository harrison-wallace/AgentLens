import { create } from "zustand";
import { clampFeedMaxEntries, DEFAULT_FEED_MAX_ENTRIES } from "../lib/feed";
import type { FsEvent } from "../lib/protocol";

/**
 * The feed renders every entry it holds, so this bound is a DOM-node budget
 * as much as a memory one: at up to 20 rendered rows per entry, 250 keeps the
 * worst case near 5k nodes. It's a live activity view, not the history — that
 * arrives with session snapshots. Override via app setting `feedMaxEntries`.
 */
const DEFAULT_MAX_ENTRIES = DEFAULT_FEED_MAX_ENTRIES;

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

/** Session-scoped file create/delete counts for the status bar. */
export interface SessionTotals {
  created: number;
  deleted: number;
}

const EMPTY_TOTALS: SessionTotals = { created: 0, deleted: 0 };

interface FeedStore {
  /** Newest-first, bounded at `maxEntries`. */
  entries: FeedEntry[];
  /** Cap applied on insert and when the setting changes. */
  maxEntries: number;
  /**
   * Running +/− for this watch session. Survives feed entry eviction so the
   * footer stays honest after the list scrolls past the cap; reset by `clear`
   * (Clear / session restart).
   */
  sessionTotals: SessionTotals;
  addBatch: (events: FsEvent[]) => void;
  /** The connection went down at `at`; start marking the window. */
  beginGap: (at: number) => void;
  /** The connection came back at `at`; close the open window, if any. */
  endGap: (at: number) => void;
  /** Apply a new cap from settings and drop oldest entries if needed. */
  setMaxEntries: (max: number) => void;
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

/** Count file creates/deletes in a batch for the session running total. */
export function countSessionDelta(events: readonly FsEvent[]): SessionTotals {
  let created = 0;
  let deleted = 0;
  for (const event of events) {
    if (event.kind === "created") created += 1;
    else if (event.kind === "deleted") deleted += 1;
  }
  return { created, deleted };
}

export const useFeedStore = create<FeedStore>((set, get) => ({
  entries: [],
  maxEntries: DEFAULT_MAX_ENTRIES,
  sessionTotals: { ...EMPTY_TOTALS },

  addBatch: (events) => {
    if (events.length === 0) return;
    const entry: FeedEntry = { kind: "batch", id: id(), at: Date.now(), events };
    const delta = countSessionDelta(events);
    const prev = get().sessionTotals;
    const max = get().maxEntries;
    set({
      entries: [entry, ...get().entries].slice(0, max),
      sessionTotals: {
        created: prev.created + delta.created,
        deleted: prev.deleted + delta.deleted,
      },
    });
  },

  beginGap: (at) => {
    const entries = get().entries;
    // Reconnect attempts can fail repeatedly; one outage is one gap, not one
    // per attempt.
    if (entries.some(isOpenGap)) return;
    const entry: FeedEntry = { kind: "gap", id: id(), at, from: at, to: null };
    set({ entries: [entry, ...entries].slice(0, get().maxEntries) });
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

  setMaxEntries: (max) => {
    const maxEntries = clampFeedMaxEntries(max);
    set({
      maxEntries,
      entries: get().entries.slice(0, maxEntries),
    });
  },

  clear: () => set({ entries: [], sessionTotals: { ...EMPTY_TOTALS } }),
}));
