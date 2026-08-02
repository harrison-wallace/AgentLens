/**
 * Pure activity-feed helpers. No React, no Tauri, no jsdom — kept plain so
 * they run under node-environment vitest.
 */
import type { FsEvent, FsEventKind } from "./protocol";

/** Default activity-feed batch cap — live window, not full history. */
export const DEFAULT_FEED_MAX_ENTRIES = 250;
/** Floor so a mis-typed setting can't collapse the feed to nothing useful. */
export const MIN_FEED_MAX_ENTRIES = 50;
/** Ceiling so a huge number can't balloon the unvirtualized DOM. */
export const MAX_FEED_MAX_ENTRIES = 2_000;

/** Clamp a raw settings value into a safe feed cap. */
export function clampFeedMaxEntries(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_FEED_MAX_ENTRIES;
  return Math.min(MAX_FEED_MAX_ENTRIES, Math.max(MIN_FEED_MAX_ENTRIES, Math.round(value)));
}

/** How the feed list is ordered. */
export type FeedSort = "time" | "created" | "deleted" | "events";

/** Per-kind counts for a set of filesystem events. */
export type KindCounts = Record<FsEventKind, number>;

/** Badge glyph shown for each change kind — matches feed row markers. */
export const KIND_BADGE: Record<FsEventKind, string> = {
  created: "+",
  modified: "M",
  deleted: "−",
  renamed: "→",
};

/** Stable display order for kind stats (matches status-bar scan order). */
export const KIND_ORDER: FsEventKind[] = ["created", "modified", "deleted", "renamed"];

const EMPTY_COUNTS: KindCounts = {
  created: 0,
  modified: 0,
  deleted: 0,
  renamed: 0,
};

/** Relative-time label for a batch header, e.g. "just now", "12s ago". */
export function groupLabel(at: number, now: number = Date.now()): string {
  const diffMs = Math.max(0, now - at);
  if (diffMs < 10_000) return "just now";
  if (diffMs < 60_000) return `${Math.floor(diffMs / 1_000)}s ago`;
  if (diffMs < 3_600_000) return `${Math.floor(diffMs / 60_000)}m ago`;
  return `${Math.floor(diffMs / 3_600_000)}h ago`;
}

/** Wall-clock `HH:MM` for a gap marker, which is about *when*, not "how long ago". */
export function clockLabel(at: number): string {
  return new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/**
 * The line a gap marker shows, e.g. "disconnected 14:02–14:03". An open gap
 * has no end yet, and saying so is the point — the app is admitting it does
 * not know what is happening right now.
 */
export function gapLabel(from: number, to: number | null): string {
  return to === null
    ? `disconnected since ${clockLabel(from)}`
    : `disconnected ${clockLabel(from)}–${clockLabel(to)}`;
}

/**
 * Split a workspace-relative path into the filename and the directory holding
 * it, so a feed row can lead with the part that identifies the file.
 *
 * No trailing-separator case to handle: `resolve_in_workspace` rejects paths
 * with leading, doubled or trailing separators, so nothing that reaches the
 * feed has one.
 */
export function splitPath(path: string): { name: string; dir: string } {
  const cut = path.lastIndexOf("/");
  return cut === -1
    ? { name: path, dir: "" }
    : { name: path.slice(cut + 1), dir: path.slice(0, cut) };
}

export function emptyKindCounts(): KindCounts {
  return { ...EMPTY_COUNTS };
}

/** Count events by kind. */
export function countByKind(events: readonly FsEvent[]): KindCounts {
  const counts = emptyKindCounts();
  for (const event of events) {
    counts[event.kind] += 1;
  }
  return counts;
}

/**
 * Aggregate kind counts across every batch in the feed. Gaps contribute nothing.
 * Totals describe the full live feed, not the active filter.
 */
export function countFeedByKind(
  entries: readonly { kind: string; events?: readonly FsEvent[] }[],
): KindCounts {
  const counts = emptyKindCounts();
  for (const entry of entries) {
    if (entry.kind !== "batch" || !entry.events) continue;
    for (const event of entry.events) {
      counts[event.kind] += 1;
    }
  }
  return counts;
}

/**
 * Compact status-bar style summary, e.g. "M 3  + 1".
 * Non-zero kinds only, highest count first; ties break on KIND_ORDER.
 */
export function summarizeBatch(events: readonly FsEvent[]): string {
  return formatKindCounts(countByKind(events));
}

/** Format kind counts as "M 3  + 1". Empty when every count is zero. */
export function formatKindCounts(counts: KindCounts): string {
  return KIND_ORDER.filter((kind) => counts[kind] > 0)
    .sort((a, b) => counts[b] - counts[a] || KIND_ORDER.indexOf(a) - KIND_ORDER.indexOf(b))
    .map((kind) => `${KIND_BADGE[kind]} ${counts[kind]}`)
    .join("  ");
}

/**
 * Non-zero kinds in display order for rendering colored stats.
 * Sort is by count desc, then KIND_ORDER — same as `formatKindCounts`.
 */
export function kindCountParts(
  counts: KindCounts,
): { kind: FsEventKind; badge: string; count: number }[] {
  return KIND_ORDER.filter((kind) => counts[kind] > 0)
    .sort((a, b) => counts[b] - counts[a] || KIND_ORDER.indexOf(a) - KIND_ORDER.indexOf(b))
    .map((kind) => ({ kind, badge: KIND_BADGE[kind], count: counts[kind] }));
}

/**
 * An empty filter set means "show everything". A non-empty set is an allow-list.
 */
export function eventMatchesFilter(kind: FsEventKind, filter: ReadonlySet<FsEventKind>): boolean {
  return filter.size === 0 || filter.has(kind);
}

export function filterEvents(
  events: readonly FsEvent[],
  filter: ReadonlySet<FsEventKind>,
): FsEvent[] {
  if (filter.size === 0) return events.slice();
  return events.filter((event) => filter.has(event.kind));
}

/** Toggle one kind in the filter; clearing the last active kind returns to "all". */
export function toggleKindFilter(
  filter: ReadonlySet<FsEventKind>,
  kind: FsEventKind,
): Set<FsEventKind> {
  const next = new Set(filter);
  if (next.has(kind)) next.delete(kind);
  else next.add(kind);
  return next;
}

/** Cycle order for the compact sort control. */
export const FEED_SORT_CYCLE: FeedSort[] = ["time", "created", "deleted", "events"];

export function nextFeedSort(current: FeedSort): FeedSort {
  const index = FEED_SORT_CYCLE.indexOf(current);
  return FEED_SORT_CYCLE[(index + 1) % FEED_SORT_CYCLE.length] ?? "time";
}

/** Short mono label for the sort control. */
export function feedSortLabel(sort: FeedSort): string {
  switch (sort) {
    case "time":
      return "time";
    case "created":
      return "most +";
    case "deleted":
      return "most −";
    case "events":
      return "most";
  }
}

export function feedSortTitle(sort: FeedSort): string {
  switch (sort) {
    case "time":
      return "Sort by time (newest first). Click to change.";
    case "created":
      return "Sort by most created. Click to change.";
    case "deleted":
      return "Sort by most deleted. Click to change.";
    case "events":
      return "Sort by most events. Click to change.";
  }
}

type BatchEntry = { kind: "batch"; id: string; at: number; events: FsEvent[] };
type GapEntry = {
  kind: "gap";
  id: string;
  at: number;
  from: number;
  to: number | null;
};
type AnyEntry = BatchEntry | GapEntry;

/**
 * Apply kind filter and sort. Gaps always survive filtering; under impact
 * sorts they sit after batches (newest gap first) so they don't scramble the
 * ranking. Under time sort the original newest-first interleave is preserved.
 */
export function presentFeedEntries(
  entries: readonly AnyEntry[],
  filter: ReadonlySet<FsEventKind>,
  sort: FeedSort,
): AnyEntry[] {
  const filtered: AnyEntry[] = [];
  for (const entry of entries) {
    if (entry.kind === "gap") {
      filtered.push(entry);
      continue;
    }
    const events = filterEvents(entry.events, filter);
    if (events.length === 0) continue;
    filtered.push(events === entry.events ? entry : { ...entry, events });
  }

  if (sort === "time") return filtered;

  const batches: BatchEntry[] = [];
  const gaps: GapEntry[] = [];
  for (const entry of filtered) {
    if (entry.kind === "gap") gaps.push(entry);
    else batches.push(entry);
  }

  const score = (events: FsEvent[]): number => {
    if (sort === "events") return events.length;
    let n = 0;
    for (const event of events) {
      if (event.kind === sort) n += 1;
    }
    return n;
  };

  batches.sort((a, b) => {
    const delta = score(b.events) - score(a.events);
    if (delta !== 0) return delta;
    return b.at - a.at;
  });

  gaps.sort((a, b) => b.at - a.at);
  return [...batches, ...gaps];
}
