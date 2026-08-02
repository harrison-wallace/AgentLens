/**
 * Pure activity-feed helpers. No React, no Tauri, no jsdom — kept plain so
 * they run under node-environment vitest.
 */
import type { FsEvent, FsEventKind } from "./protocol";

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

const KIND_ORDER: FsEventKind[] = ["created", "modified", "deleted", "renamed"];

/** Short summary line for a batch, e.g. "3 modified, 1 created". */
export function summarizeBatch(events: FsEvent[]): string {
  const counts: Record<FsEventKind, number> = {
    created: 0,
    modified: 0,
    deleted: 0,
    renamed: 0,
  };
  for (const event of events) {
    counts[event.kind] += 1;
  }

  return KIND_ORDER.filter((kind) => counts[kind] > 0)
    .sort((a, b) => counts[b] - counts[a])
    .map((kind) => `${counts[kind]} ${kind}`)
    .join(", ");
}
