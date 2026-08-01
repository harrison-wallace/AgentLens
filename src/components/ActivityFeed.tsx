import { useEffect, useState } from "react";
import { useFeedStore, type FeedEntry } from "../stores/feedStore";
import { useTreeStore } from "../stores/treeStore";
import { groupLabel, summarizeBatch } from "../lib/feed";
import type { FsEventKind } from "../lib/protocol";

/** Render at most this many rows per batch; the rest collapse to "+N more". */
const MAX_ROWS_PER_BATCH = 20;
/** How often relative-time headers ("12s ago") refresh. */
const TICK_INTERVAL_MS = 15_000;

const KIND_BADGE: Record<FsEventKind, string> = {
  created: "+",
  modified: "M",
  deleted: "−",
  renamed: "→",
};

const KIND_CLASS: Record<FsEventKind, string> = {
  created: "text-git-added",
  modified: "text-git-modified",
  deleted: "text-git-deleted",
  renamed: "text-git-renamed",
};

export default function ActivityFeed() {
  const entries = useFeedStore((s) => s.entries);
  const revealPath = useTreeStore((s) => s.revealPath);

  // Forces a re-render every so often so "just now" / "12s ago" headers
  // stay roughly accurate without a timer per entry.
  const [, setTick] = useState(0);
  useEffect(() => {
    const id = setInterval(() => setTick((t) => t + 1), TICK_INTERVAL_MS);
    return () => clearInterval(id);
  }, []);

  if (entries.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-4 text-center text-xs text-text-muted">
        No activity yet.
      </div>
    );
  }

  return (
    <div className="h-full min-h-0 overflow-y-auto">
      {entries.map((entry) => (
        <FeedBlock key={entry.id} entry={entry} onSelect={(path) => void revealPath(path)} />
      ))}
    </div>
  );
}

function FeedBlock({ entry, onSelect }: { entry: FeedEntry; onSelect: (path: string) => void }) {
  const shown = entry.events.slice(0, MAX_ROWS_PER_BATCH);
  const hidden = entry.events.length - shown.length;

  return (
    <div className="border-b border-border px-3 py-2">
      <div className="flex items-baseline justify-between gap-2">
        <span className="shrink-0 text-xs text-text-muted">{groupLabel(entry.at)}</span>
        <span className="min-w-0 truncate text-xs text-text-muted">
          {summarizeBatch(entry.events)}
        </span>
      </div>
      <ul className="mt-1 flex flex-col gap-0.5">
        {shown.map((event, i) => (
          <li key={`${event.path}:${i}`}>
            <button
              type="button"
              onClick={() => onSelect(event.path)}
              title={event.path}
              className="flex w-full items-center gap-2 truncate rounded px-1 py-0.5 text-left text-sm text-text hover:bg-hover"
            >
              <span
                className={`w-3 shrink-0 text-center font-mono text-xs ${KIND_CLASS[event.kind]}`}
              >
                {KIND_BADGE[event.kind]}
              </span>
              <span className="min-w-0 flex-1 truncate">{event.path}</span>
            </button>
          </li>
        ))}
      </ul>
      {hidden > 0 && <p className="mt-1 px-1 text-xs text-text-muted">+{hidden} more</p>}
    </div>
  );
}
