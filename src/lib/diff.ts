/**
 * Line diff for the "Diff since session" tab. Wraps the `diff` package into
 * flat rows the UI can render directly, so the component has no diff logic
 * of its own and this stays testable without a DOM.
 */
import { diffLines } from "diff";
import type { SessionDiff } from "./protocol";

export type DiffRowKind = "added" | "removed" | "context";

export interface DiffRow {
  kind: DiffRowKind;
  text: string;
  /** 1-based line number on the baseline side, or null for added lines. */
  baselineLine: number | null;
  /** 1-based line number on the current side, or null for removed lines. */
  currentLine: number | null;
}

export interface DiffSummary {
  added: number;
  removed: number;
}

/**
 * Split a chunk into lines without inventing a trailing empty line for the
 * final newline — that would show up as a phantom row in every diff.
 */
function toLines(value: string): string[] {
  const lines = value.split("\n");
  if (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

/** Flatten a baseline/current pair into renderable rows. */
export function toDiffRows(baseline: string | null, current: string | null): DiffRow[] {
  const changes = diffLines(baseline ?? "", current ?? "");
  const rows: DiffRow[] = [];
  let baselineLine = 1;
  let currentLine = 1;

  for (const change of changes) {
    for (const text of toLines(change.value)) {
      if (change.added) {
        rows.push({ kind: "added", text, baselineLine: null, currentLine: currentLine++ });
      } else if (change.removed) {
        rows.push({ kind: "removed", text, baselineLine: baselineLine++, currentLine: null });
      } else {
        rows.push({
          kind: "context",
          text,
          baselineLine: baselineLine++,
          currentLine: currentLine++,
        });
      }
    }
  }
  return rows;
}

/** Added/removed line counts, for the tab header. */
export function summarizeDiff(rows: DiffRow[]): DiffSummary {
  let added = 0;
  let removed = 0;
  for (const row of rows) {
    if (row.kind === "added") added += 1;
    else if (row.kind === "removed") removed += 1;
  }
  return { added, removed };
}

/**
 * Collapse long stretches of unchanged lines, keeping `context` rows either
 * side of each change. Returns rows interleaved with gap markers.
 */
export type DiffDisplayRow = { type: "row"; row: DiffRow } | { type: "gap"; hidden: number };

export function collapseContext(rows: DiffRow[], context: number): DiffDisplayRow[] {
  const keep = new Array<boolean>(rows.length).fill(false);
  rows.forEach((row, index) => {
    if (row.kind === "context") return;
    for (
      let i = Math.max(0, index - context);
      i <= Math.min(rows.length - 1, index + context);
      i++
    ) {
      keep[i] = true;
    }
  });

  const out: DiffDisplayRow[] = [];
  let hidden = 0;
  rows.forEach((row, index) => {
    if (keep[index]) {
      if (hidden > 0) {
        out.push({ type: "gap", hidden });
        hidden = 0;
      }
      out.push({ type: "row", row });
    } else {
      hidden += 1;
    }
  });
  if (hidden > 0) out.push({ type: "gap", hidden });
  return out;
}

/**
 * Cap on rendered diff rows. `collapseContext` only elides *unchanged* runs,
 * so a file rewritten wholesale keeps every line — near the 1 MB baseline cap
 * that is tens of thousands of DOM rows, which locks the pane. A rewrite that
 * large isn't readable as a diff anyway.
 */
export const MAX_DIFF_ROWS = 2_000;

export interface TruncatedDisplay {
  display: DiffDisplayRow[];
  /** Rows dropped from the end; 0 when the whole diff fits. */
  truncated: number;
}

export function truncateDisplay(display: DiffDisplayRow[], max: number): TruncatedDisplay {
  if (display.length <= max) return { display, truncated: 0 };
  return { display: display.slice(0, max), truncated: display.length - max };
}

/** Human-readable reason the diff can't be shown, or null if it can. */
export function diffUnavailableReason(diff: SessionDiff): string | null {
  switch (diff.unavailable) {
    case "notARepository":
      return "Diff since session needs a git repository — the baseline comes from git.";
    case "notTracked":
      return "Git ignores this file, so there is no session baseline to compare against.";
    case null:
      break;
  }
  if (diff.baseline === null && diff.current === null) {
    return "No content to compare: the file is missing, binary, or over 1 MB.";
  }
  return null;
}
