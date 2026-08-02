import { useEffect, useMemo, useState } from "react";
import { useFeedStore, type FeedEntry } from "../stores/feedStore";
import { useTreeStore } from "../stores/treeStore";
import {
  countByKind,
  countFeedByKind,
  feedSortLabel,
  feedSortTitle,
  gapLabel,
  groupLabel,
  KIND_BADGE,
  KIND_ORDER,
  kindCountParts,
  nextFeedSort,
  presentFeedEntries,
  splitPath,
  type FeedSort,
} from "../lib/feed";
import type { FsEvent, FsEventKind } from "../lib/protocol";

/** Render at most this many rows per batch; the rest collapse to "+N more". */
const MAX_ROWS_PER_BATCH = 20;
/** How often relative-time headers ("12s ago") refresh. */
const TICK_INTERVAL_MS = 15_000;

const KIND_CLASS: Record<FsEventKind, string> = {
  created: "text-git-added",
  modified: "text-git-modified",
  deleted: "text-git-deleted",
  renamed: "text-git-renamed",
};

const KIND_LABEL: Record<FsEventKind, string> = {
  created: "created",
  modified: "modified",
  deleted: "deleted",
  renamed: "renamed",
};

export default function ActivityFeed() {
  const entries = useFeedStore((s) => s.entries);
  const revealPath = useTreeStore((s) => s.revealPath);
  const [filter, setFilter] = useState<Set<FsEventKind>>(() => new Set());
  const [sort, setSort] = useState<FeedSort>("time");

  // Forces a re-render every so often so "just now" / "12s ago" headers
  // stay roughly accurate without a timer per entry.
  const [, setTick] = useState(0);
  useEffect(() => {
    const id = setInterval(() => setTick((t) => t + 1), TICK_INTERVAL_MS);
    return () => clearInterval(id);
  }, []);

  const totals = useMemo(() => countFeedByKind(entries), [entries]);
  const presented = useMemo(
    () => presentFeedEntries(entries, filter, sort),
    [entries, filter, sort],
  );

  const toggleKind = (kind: FsEventKind) => {
    setFilter((prev) => {
      const next = new Set(prev);
      if (next.has(kind)) next.delete(kind);
      else next.add(kind);
      return next;
    });
  };

  if (entries.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-4 text-center text-xs text-text-muted">
        No activity yet.
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <FeedToolbar
        totals={totals}
        filter={filter}
        sort={sort}
        onToggleKind={toggleKind}
        onCycleSort={() => setSort((prev) => nextFeedSort(prev))}
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        {presented.length === 0 ? (
          <div className="flex h-full items-center justify-center p-4 text-center text-xs text-text-muted">
            No matching activity.
          </div>
        ) : (
          presented.map((entry) =>
            entry.kind === "gap" ? (
              <GapMarker key={entry.id} entry={entry} />
            ) : (
              <FeedBlock key={entry.id} entry={entry} onSelect={(path) => void revealPath(path)} />
            ),
          )
        )}
      </div>
    </div>
  );
}

/**
 * Compact status-bar twin: kind counts as filters, mono sort on the right.
 * Totals are always for the full live feed, not the active filter.
 */
function FeedToolbar({
  totals,
  filter,
  sort,
  onToggleKind,
  onCycleSort,
}: {
  totals: Record<FsEventKind, number>;
  filter: ReadonlySet<FsEventKind>;
  sort: FeedSort;
  onToggleKind: (kind: FsEventKind) => void;
  onCycleSort: () => void;
}) {
  const filtering = filter.size > 0;

  return (
    <div className="flex h-7 shrink-0 items-center gap-3 border-b border-border px-3 text-[11px] text-text-muted">
      <span className="flex min-w-0 items-center gap-3 tabular-nums">
        {KIND_ORDER.map((kind) => {
          const count = totals[kind];
          const active = filter.has(kind);
          // When nothing is filtered every kind is "on"; once a filter is set,
          // only selected kinds keep full color — the rest mute so the strip
          // still shows the full feed totals without looking selected.
          const emphasis = !filtering || active;
          return (
            <button
              key={kind}
              type="button"
              onClick={() => onToggleKind(kind)}
              aria-pressed={filtering ? active : false}
              title={
                active
                  ? `Showing ${KIND_LABEL[kind]} only — click to clear`
                  : filtering
                    ? `Also show ${KIND_LABEL[kind]}`
                    : `Show only ${KIND_LABEL[kind]}`
              }
              className={`shrink-0 rounded px-0.5 hover:bg-hover ${
                emphasis ? KIND_CLASS[kind] : "text-text-muted opacity-50"
              } ${active ? "underline decoration-current underline-offset-2" : ""}`}
            >
              {KIND_BADGE[kind]} {count}
            </button>
          );
        })}
      </span>
      <button
        type="button"
        onClick={onCycleSort}
        title={feedSortTitle(sort)}
        aria-label={feedSortTitle(sort)}
        className="ml-auto shrink-0 rounded px-0.5 hover:bg-hover hover:text-text"
      >
        {feedSortLabel(sort)}
      </button>
    </div>
  );
}

/**
 * The window in which the app was not being told about changes.
 *
 * Deliberately loud: silently resuming the feed after an outage would claim
 * nothing happened while the link was down, which is the one thing that
 * cannot be known.
 */
function GapMarker({ entry }: { entry: Extract<FeedEntry, { kind: "gap" }> }) {
  const open = entry.to === null;
  return (
    <div
      role="status"
      className={`flex items-center gap-2 border-b border-border px-3 py-2 text-xs ${
        open ? "text-danger" : "text-text-muted"
      }`}
    >
      <span aria-hidden className="h-px flex-1 bg-border" />
      <span className="shrink-0">{gapLabel(entry.from, entry.to)}</span>
      <span aria-hidden className="h-px flex-1 bg-border" />
    </div>
  );
}

function FeedBlock({
  entry,
  onSelect,
}: {
  entry: Extract<FeedEntry, { kind: "batch" }>;
  onSelect: (path: string) => void;
}) {
  const shown = entry.events.slice(0, MAX_ROWS_PER_BATCH);
  const hidden = entry.events.length - shown.length;
  const parts = kindCountParts(countByKind(entry.events));

  return (
    <div className="border-b border-border px-3 py-2">
      <div className="flex items-baseline justify-between gap-2">
        <span className="shrink-0 text-[11px] text-text-muted">{groupLabel(entry.at)}</span>
        <span className="flex min-w-0 items-center justify-end gap-2 overflow-hidden text-[11px] tabular-nums">
          {parts.map(({ kind, badge, count }) => (
            <span key={kind} className={`shrink-0 ${KIND_CLASS[kind]}`}>
              {badge} {count}
            </span>
          ))}
        </span>
      </div>
      <ul className="mt-1 flex flex-col gap-0.5">
        {shown.map((event, i) => (
          <li key={`${event.path}:${i}`}>
            <EventRow event={event} onSelect={onSelect} />
          </li>
        ))}
      </ul>
      {hidden > 0 && <p className="mt-1 px-1 text-[11px] text-text-muted">+{hidden} more</p>}
    </div>
  );
}

/**
 * One changed file: name first, then the directory holding it.
 *
 * The panel is rarely wide enough for a full path, and a single truncated
 * string loses its tail — which is the filename, the only part that says
 * *which* file this is. Two spans let the directory absorb the truncation
 * instead, from the left, so deep sibling paths stop rendering identically.
 */
function EventRow({ event, onSelect }: { event: FsEvent; onSelect: (path: string) => void }) {
  const { name, dir } = splitPath(event.path);

  return (
    <button
      type="button"
      onClick={() => onSelect(event.path)}
      title={event.path}
      className="flex w-full items-baseline gap-2 px-1 py-0.5 text-left text-xs hover:bg-hover"
    >
      <span
        className={`w-3 shrink-0 self-center text-center text-[11px] tabular-nums ${KIND_CLASS[event.kind]}`}
      >
        {KIND_BADGE[event.kind]}
      </span>
      {/* The name gets the space it needs and truncates only when it alone
          overruns the row; whatever is left over goes to the directory. */}
      <span className="min-w-0 shrink truncate text-text-body">{name}</span>
      {dir && (
        <span className="truncate-start min-w-0 flex-1 text-[11px] text-text-muted">
          <bdi>{dir}</bdi>
        </span>
      )}
    </button>
  );
}
