/**
 * Pure tree-flattening and git-decoration helpers. No React, no Tauri — kept
 * cheap and deterministic since `FileTree` calls `flattenTree` on every
 * render to feed the virtualizer.
 */
import type { DirEntryNode, GitFileStatus, GitStatusKind } from "./protocol";

export interface TreeRow {
  name: string;
  path: string;
  isDir: boolean;
  depth: number;
  /** Git ignores this entry; the tree dims it. */
  ignored: boolean;
  /** A file that instructs a coding agent; the tree marks it. */
  agentContext: boolean;
}

/**
 * Walk from the workspace root (`""`) emitting one row per loaded child, in
 * backend order, recursing into expanded directories. A directory that is
 * expanded but has no cached children yet still gets its own row — it just
 * has nothing under it until `loadDir` resolves.
 */
export function flattenTree(
  childrenByPath: Record<string, DirEntryNode[]>,
  expanded: Set<string>,
): TreeRow[] {
  const rows: TreeRow[] = [];

  const walk = (path: string, depth: number): void => {
    const children = childrenByPath[path];
    if (!children) return;
    for (const child of children) {
      rows.push({
        name: child.name,
        path: child.path,
        isDir: child.isDir,
        depth,
        ignored: child.ignored,
        agentContext: child.agentContext,
      });
      if (child.isDir && expanded.has(child.path)) {
        walk(child.path, depth + 1);
      }
    }
  };

  walk("", 0);
  return rows;
}

const BADGES: Record<GitStatusKind, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  untracked: "U",
  conflicted: "!",
};

/** Single-letter git badge for `path`, or `null` if it has no status. */
export function gitBadgeFor(
  path: string,
  statusByPath: Record<string, GitStatusKind>,
): string | null {
  const kind = statusByPath[path];
  return kind ? BADGES[kind] : null;
}

export interface GitCounts {
  modified: number;
  added: number;
  deleted: number;
  untracked: number;
}

/**
 * Distinct parent directories of `paths`, in first-seen order. A root-level
 * path (no `/`) maps to `""`, the workspace root.
 */
export function parentDirsOf(paths: string[]): string[] {
  const seen = new Set<string>();
  const dirs: string[] = [];
  for (const path of paths) {
    const slash = path.lastIndexOf("/");
    const dir = slash === -1 ? "" : path.slice(0, slash);
    if (!seen.has(dir)) {
      seen.add(dir);
      dirs.push(dir);
    }
  }
  return dirs;
}

/**
 * Counts for the status bar. Renamed/conflicted files aren't broken out.
 *
 * Counts *files*, not status entries: a path with both staged and unstaged
 * changes contributes two entries and must still count once, or the status
 * bar overstates how much has changed.
 */
export function countsFor(files: GitFileStatus[]): GitCounts {
  const counts: GitCounts = { modified: 0, added: 0, deleted: 0, untracked: 0 };
  const seen = new Set<string>();
  for (const file of files) {
    if (seen.has(file.path)) continue;
    seen.add(file.path);
    switch (file.status) {
      case "modified":
        counts.modified += 1;
        break;
      case "added":
        counts.added += 1;
        break;
      case "deleted":
        counts.deleted += 1;
        break;
      case "untracked":
        counts.untracked += 1;
        break;
      default:
        break;
    }
  }
  return counts;
}
