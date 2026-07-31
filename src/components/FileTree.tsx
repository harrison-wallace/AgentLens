import { useMemo, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useGitStore } from "../stores/gitStore";
import { useTreeStore } from "../stores/treeStore";
import { flattenTree, gitBadgeFor, type TreeRow } from "../lib/treeRows";
import type { GitStatusKind } from "../lib/protocol";

const ROW_HEIGHT = 24;

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
  const statusByPath = useGitStore((s) => s.statusByPath);

  const parentRef = useRef<HTMLDivElement>(null);

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

  return (
    <div ref={parentRef} className="h-full min-h-0 overflow-y-auto">
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
          const statusKind = statusByPath[row.path];
          const badge = gitBadgeFor(row.path, statusByPath);

          return (
            <button
              key={row.path}
              type="button"
              onClick={() => (row.isDir ? toggle(row.path) : select(row.path))}
              title={row.path}
              style={{ ...commonStyle, paddingLeft: row.depth * 14 + 8 }}
              className={`flex items-center gap-1 truncate pr-2 text-left text-sm ${
                isSelected ? "bg-selected text-text" : "text-text hover:bg-hover"
              }`}
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
              <span className={`min-w-0 flex-1 truncate ${isLoading ? "opacity-50" : ""}`}>
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
