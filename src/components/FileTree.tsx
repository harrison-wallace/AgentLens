import { useEffect, useMemo, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useGitStore } from "../stores/gitStore";
import { usePreviewStore } from "../stores/previewStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useTreeStore } from "../stores/treeStore";
import { flattenTree, gitBadgeFor, rollUpHidden, type TreeRow } from "../lib/treeRows";
import type { GitStatusKind, PinnedEntry } from "../lib/protocol";

const ROW_HEIGHT = 24;
/** How often the single glow-pruning timer ticks. */
const GLOW_PRUNE_INTERVAL_MS = 5_000;
/** Filled once pinned, hollow as the hover affordance. */
const PIN_ON = "✦";
const PIN_OFF = "✧";

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
  const openTabs = usePreviewStore((s) => s.tabs);
  const openPaths = useMemo(() => new Set(openTabs.map((t) => t.path)), [openTabs]);
  const pinnedPaths = useSettingsStore((s) => s.settings.pinned);
  const togglePin = useSettingsStore((s) => s.togglePin);

  const collapse = useTreeStore((s) => s.collapse);
  const parentRef = useRef<HTMLDivElement>(null);

  /** Tree cursor + VS Code-style preview open (single-click / arrow). */
  const selectFilePreview = (path: string) => {
    select(path, false);
    void usePreviewStore.getState().openPreview(path);
  };

  /** Keep the tab (double-click / Enter). */
  const selectFilePermanent = (path: string) => {
    select(path, false);
    void usePreviewStore.getState().openPermanent(path);
  };

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

  const pinned = useMemo(() => new Set(pinnedPaths), [pinnedPaths]);

  // Both decorations answer the same question for a collapsed subtree — "what
  // is in there that I can't see" — so they share one roll-up. Keyed off
  // `displayRows`, which only changes on expand/collapse/load, never on scroll.
  const visiblePaths = useMemo(
    () => new Set(displayRows.flatMap((d) => (d.kind === "node" ? [d.row.path] : []))),
    [displayRows],
  );
  const hiddenChanges = useMemo(
    () => rollUpHidden(Object.keys(recentlyChanged), visiblePaths),
    [recentlyChanged, visiblePaths],
  );
  const hiddenStatus = useMemo(
    () => rollUpHidden(Object.keys(statusByPath), visiblePaths),
    [statusByPath, visiblePaths],
  );

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
    if (target.row.isDir) select(target.row.path, true);
    else selectFilePreview(target.row.path);
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
        else if (row) selectFilePermanent(row.path);
        break;
      // Bare `p`: the tree is not a text field, and `Ctrl+P` is the file jump.
      case "p":
      case "P":
        if (event.ctrlKey || event.metaKey || !row) break;
        event.preventDefault();
        void togglePin(row.path);
        break;
      default:
        break;
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PinnedGroup />
      <div
        ref={parentRef}
        tabIndex={0}
        onKeyDown={onKeyDown}
        role="tree"
        aria-label="File tree"
        className="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable] focus:outline-none"
      >
        <div
          role="presentation"
          style={{ height: virtualizer.getTotalSize(), position: "relative" }}
        >
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
                  role="none"
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
            const isOpen = !row.isDir && openPaths.has(row.path);
            const isRecentlyChanged = row.path in recentlyChanged;
            const isPinned = pinned.has(row.path);
            const statusKind = statusByPath[row.path];
            const badge = gitBadgeFor(row.path, statusByPath);
            // Only ever non-zero on a row hiding something: a row whose own
            // path is decorated shows that decoration instead.
            const changedInside = hiddenChanges[row.path] ?? 0;
            const statusInside = hiddenStatus[row.path] ?? 0;

            return (
              <div
                key={row.path}
                role="treeitem"
                aria-level={row.depth + 1}
                aria-selected={isSelected}
                aria-expanded={row.isDir ? isExpanded : undefined}
                onClick={() => {
                  if (row.isDir) {
                    select(row.path, true);
                    toggle(row.path);
                  } else {
                    selectFilePreview(row.path);
                  }
                }}
                onDoubleClick={() => {
                  if (!row.isDir) selectFilePermanent(row.path);
                }}
                title={row.path}
                style={{ ...commonStyle, paddingLeft: row.depth * 14 + 8 }}
                className={`group flex cursor-default items-center gap-1 pr-4 text-left text-xs ${
                  isSelected ? "bg-selected text-text" : "text-text-body hover:bg-hover"
                } ${
                  isRecentlyChanged
                    ? "tree-row-glow"
                    : changedInside > 0
                      ? "tree-row-glow-proxy"
                      : ""
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
                <span
                  className={`min-w-0 flex-1 truncate ${isLoading ? "opacity-50" : ""} ${
                    row.ignored ? "text-text-ash" : ""
                  }`}
                  title={row.ignored ? `${row.path} — ignored by git` : undefined}
                >
                  {row.name}
                </span>
                {isOpen && (
                  <span
                    className="shrink-0 text-[11px] text-accent"
                    title="Open in preview"
                    aria-label="Open in preview"
                  >
                    ●
                  </span>
                )}
                {row.agentContext && (
                  <span
                    className="shrink-0 text-[11px] text-accent"
                    title="Agent context file — always shown"
                    aria-label="Agent context file"
                  >
                    ◆
                  </span>
                )}
                {changedInside > 0 && (
                  <span
                    className="shrink-0 text-[11px] tabular-nums text-glow"
                    title={`${changedInside} recently changed ${
                      changedInside === 1 ? "file" : "files"
                    } inside — expand to see ${changedInside === 1 ? "it" : "them"}`}
                    aria-label={`${changedInside} recently changed inside`}
                  >
                    ●{changedInside}
                  </span>
                )}
                {isLoading && <span className="shrink-0 text-[11px] text-text-muted">…</span>}
                {/* The badge slot takes a letter for this row's own git status,
                    and a count for the statuses collapsed underneath it. A row
                    can't have both, so they can share the slot. */}
                {!isLoading && badge && statusKind && (
                  <span className={`shrink-0 text-[11px] tabular-nums ${BADGE_CLASS[statusKind]}`}>
                    {badge}
                  </span>
                )}
                {!isLoading && !badge && statusInside > 0 && (
                  <span
                    className="shrink-0 text-[11px] tabular-nums text-text-muted"
                    title={`${statusInside} changed ${
                      statusInside === 1 ? "file" : "files"
                    } inside, per git`}
                    aria-label={`${statusInside} changed inside`}
                  >
                    {statusInside}
                  </span>
                )}
                <PinButton path={row.path} pinned={isPinned} />
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

/**
 * The pin toggle on a tree row. It has to live here rather than in the preview
 * pane: the motivating case is pinning a *directory*, and only the tree
 * renders those.
 */
function PinButton({ path, pinned }: { path: string; pinned: boolean }) {
  const togglePin = useSettingsStore((s) => s.togglePin);

  return (
    <button
      type="button"
      tabIndex={-1}
      onClick={(event) => {
        // The row underneath would otherwise select and expand.
        event.stopPropagation();
        void togglePin(path);
      }}
      title={pinned ? `Unpin ${path}` : `Pin ${path} — keeps it visible and at the top`}
      aria-label={pinned ? `Unpin ${path}` : `Pin ${path}`}
      aria-pressed={pinned}
      className={`shrink-0 px-1 text-[11px] hover:text-accent ${
        pinned
          ? "text-accent"
          : "text-text-muted opacity-0 group-hover:opacity-100 focus:opacity-100"
      }`}
    >
      {pinned ? PIN_ON : PIN_OFF}
    </button>
  );
}

/**
 * The pinned paths, above the tree rather than scrolling with it — they are
 * the things you want one click away regardless of where the tree is.
 */
function PinnedGroup() {
  const pins = useSettingsStore((s) => s.pins);
  const togglePin = useSettingsStore((s) => s.togglePin);
  const selected = useTreeStore((s) => s.selected);

  if (pins.length === 0) return null;

  const open = (entry: PinnedEntry) => {
    if (!entry.exists) return;
    // Expanding the ancestors is what makes a pinned directory usable: the
    // group is a shortcut into the tree, not a second tree.
    void useTreeStore.getState().revealPath(entry.path, entry.isDir);
    if (!entry.isDir) void usePreviewStore.getState().openPermanent(entry.path);
  };

  return (
    <div className="max-h-40 shrink-0 overflow-y-auto border-b border-border pb-1 [scrollbar-gutter:stable]">
      <h2 className="section-label px-2 pb-0.5 pt-1.5">Pinned</h2>
      <ul>
        {pins.map((entry) => (
          <li key={entry.path}>
            <div
              className={`flex items-center gap-1 pl-2 pr-4 text-xs ${
                selected === entry.path ? "bg-selected" : "hover:bg-hover"
              }`}
              style={{ height: ROW_HEIGHT }}
            >
              <button
                type="button"
                onClick={() => open(entry)}
                disabled={!entry.exists}
                title={entry.exists ? entry.path : `${entry.path} — no longer exists`}
                className={`min-w-0 flex-1 truncate text-left ${
                  entry.exists ? "text-text-body" : "text-text-ash line-through"
                }`}
              >
                {entry.name}
                <span className="ml-1.5 truncate text-[11px] text-text-muted">{entry.path}</span>
              </button>
              <button
                type="button"
                onClick={() => void togglePin(entry.path)}
                title={`Unpin ${entry.path}`}
                aria-label={`Unpin ${entry.path}`}
                className="shrink-0 px-1 text-[11px] text-accent hover:text-text"
              >
                {PIN_ON}
              </button>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
