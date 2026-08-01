import { useEffect, useMemo, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useGitStore } from "../stores/gitStore";
import { useTreeStore } from "../stores/treeStore";
import { flattenTree, gitBadgeFor, type TreeRow } from "../lib/treeRows";
import type { GitStatusKind } from "../lib/protocol";

const ROW_HEIGHT = 24;
/** How often the single glow-pruning timer ticks. */
const GLOW_PRUNE_INTERVAL_MS = 5_000;

type DisplayRow =
  { kind: "node"; row: TreeRow } | { kind: "error"; path: string; depth: number; message: string };

const BADGE_CLASS: Record<GitStatusKind, string> = {
  added: "text-git-added",
  modified: "text-git-modified",
  deleted: "text-git-deleted",
  renamed: "text-git-renamed",
  untracked: "text-git-untracked",
  conflicted: "text-git-conflicted",
};

export default function FileTree() {
  const childrenByPath = useTreeStore((s) => s.childrenByPath);
  const expanded = useTreeStore((s) => s.expanded);
  const loading = useTreeStore((s) => s.loading);
  const errors = useTreeStore((s) => s.errors);
  const selected = useTreeStore((s) => s.selected);
  const toggle = useTreeStore((s) => s.toggle);
  const select = useTreeStore((s) => s.select);
  const recentlyChanged = useTreeStore((s) => s.recentlyChanged);
  const statusByPath = useGitStore((s) => s.statusByPath);

  const collapse = useTreeStore((s) => s.collapse);
  const parentRef = useRef<HTMLDivElement>(null);

  // One timer for the whole tree, not one per row: sweep expired glows on an
  // interval rather than scheduling a timeout per changed path.
  useEffect(() => {
    const id = setInterval(() => {
      useTreeStore.getState().pruneGlow(Date.now());
    }, GLOW_PRUNE_INTERVAL_MS);
    return () => clearInterval(id);
  }, []);

  // Scrolling re-renders this component on every frame, so the row list must
  // not be rebuilt from the whole tree each time.
  const displayRows: DisplayRow[] = useMemo(() => {
    const rows: DisplayRow[] = [];
    for (const row of flattenTree(childrenByPath, expanded)) {
      rows.push({ kind: "node", row });
      const error = row.isDir && expanded.has(row.path) ? errors[row.path] : undefined;
      if (error) {
        rows.push({ kind: "error", path: row.path, depth: row.depth + 1, message: error });
      }
    }
    return rows;
  }, [childrenByPath, expanded, errors]);

  const virtualizer = useVirtualizer({
    count: displayRows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
  });

  /** Index of the cursor row, or -1 when nothing in view is selected. */
  const cursor = useMemo(
    () => displayRows.findIndex((d) => d.kind === "node" && d.row.path === selected),
    [displayRows, selected],
  );

  // Selection can move without the tree being touched — "reveal in tree" from
  // the activity feed, or a `Ctrl+P` jump — and those must scroll the row into
  // view, not merely highlight it off-screen. `align: "auto"` makes this a
  // no-op when the row is already visible, so ordinary clicks don't jump.
  useEffect(() => {
    if (cursor >= 0) virtualizer.scrollToIndex(cursor, { align: "auto" });
  }, [cursor, virtualizer]);

  const moveCursor = (delta: number) => {
    // Only node rows are navigable; inline error rows are skipped over.
    const nodes = displayRows.flatMap((d, index) => (d.kind === "node" ? [index] : []));
    if (nodes.length === 0) return;
    const at = nodes.indexOf(cursor);
    const next = at === -1 ? nodes[0] : nodes[Math.min(nodes.length - 1, Math.max(0, at + delta))];
    if (next === undefined) return;
    const target = displayRows[next];
    if (target?.kind !== "node") return;
    select(target.row.path, target.row.isDir);
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    const current = cursor === -1 ? undefined : displayRows[cursor];
    const row = current?.kind === "node" ? current.row : undefined;

    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        moveCursor(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        moveCursor(-1);
        break;
      case "ArrowRight":
        event.preventDefault();
        // Expand a closed directory; on an open one, step into its first child.
        if (row?.isDir && !expanded.has(row.path)) toggle(row.path);
        else if (row?.isDir) moveCursor(1);
        break;
      case "ArrowLeft":
        event.preventDefault();
        // Collapse an open directory, otherwise jump up to the parent.
        if (row?.isDir && expanded.has(row.path)) {
          collapse(row.path);
        } else if (row) {
          const slash = row.path.lastIndexOf("/");
          if (slash !== -1) select(row.path.slice(0, slash), true);
        }
        break;
      case "Enter":
        event.preventDefault();
        if (row?.isDir) toggle(row.path);
        else if (row) select(row.path, false);
        break;
      default:
        break;
    }
  };

  return (
    <div
      ref={parentRef}
      tabIndex={0}
      onKeyDown={onKeyDown}
      aria-label="File tree"
      className="h-full min-h-0 overflow-y-auto focus:outline-none"
    >
      <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
        {virtualizer.getVirtualItems().map((item) => {
          const display = displayRows[item.index];
          if (!display) return null;

          const commonStyle = {
            position: "absolute" as const,
            top: 0,
            left: 0,
            width: "100%",
            height: ROW_HEIGHT,
            transform: `translateY(${item.start}px)`,
          };

          if (display.kind === "error") {
            return (
              <div
                key={`error:${display.path}`}
                style={{ ...commonStyle, paddingLeft: display.depth * 14 + 8 }}
                className="flex items-center truncate text-xs text-danger"
                title={display.message}
              >
                {display.message}
              </div>
            );
          }

          const row = display.row;
          const isExpanded = row.isDir && expanded.has(row.path);
          const isLoading = row.isDir && loading.has(row.path);
          const isSelected = selected === row.path;
          const isRecentlyChanged = row.path in recentlyChanged;
          const statusKind = statusByPath[row.path];
          const badge = gitBadgeFor(row.path, statusByPath);

          return (
            <button
              key={row.path}
              type="button"
              tabIndex={-1}
              onClick={() => {
                select(row.path, row.isDir);
                if (row.isDir) toggle(row.path);
              }}
              title={row.path}
              style={{ ...commonStyle, paddingLeft: row.depth * 14 + 8 }}
              className={`flex items-center gap-1 truncate pr-2 text-left text-sm ${
                isSelected ? "bg-selected text-text" : "text-text hover:bg-hover"
              } ${isRecentlyChanged ? "tree-row-glow" : ""}`}
            >
              {row.isDir ? (
                <span
                  className={`inline-block w-3 shrink-0 text-text-muted transition-transform ${
                    isExpanded ? "rotate-90" : ""
                  }`}
                >
                  ▸
                </span>
              ) : (
                <span className="inline-block w-3 shrink-0" />
              )}
              <span
                className={`min-w-0 flex-1 truncate ${isLoading ? "opacity-50" : ""} ${
                  row.ignored ? "italic text-text-muted" : ""
                }`}
                title={row.ignored ? `${row.path} — ignored by git` : undefined}
              >
                {row.name}
              </span>
              {isLoading && <span className="shrink-0 text-xs text-text-muted">…</span>}
              {!isLoading && badge && statusKind && (
                <span className={`shrink-0 font-mono text-xs ${BADGE_CLASS[statusKind]}`}>
                  {badge}
                </span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
