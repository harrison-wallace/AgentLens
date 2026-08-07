import { useEffect, useState } from "react";
import BranchControl from "./BranchControl";
import { DEFAULT_ZOOM, useAppearanceStore } from "../stores/appearanceStore";
import { useConnectionStore } from "../stores/connectionStore";
import { useFeedStore } from "../stores/feedStore";
import { useGitStore } from "../stores/gitStore";
import { useWatcherStore } from "../stores/watcherStore";
import { countsFor } from "../lib/treeRows";
import { getAppInfo } from "../lib/tauri";
import type { ConnectionState, WatcherState } from "../lib/protocol";

// Exhaustive over `WatcherState` with no `default`, so adding a state is a
// type error here rather than a silent "watcher: off".
function watcherLabel(state: WatcherState, message: string | null): string {
  switch (state) {
    case "running":
      return "watcher: on";
    case "error":
      return `watcher: error${message ? ` (${message})` : ""}`;
    case "off":
      return "watcher: off";
  }
}

// Exhaustive for the same reason as `watcherLabel`.
function connectionClass(state: ConnectionState): string {
  switch (state) {
    case "connected":
      return "text-text-muted";
    case "connecting":
    case "installing":
      return "text-git-modified";
    case "disconnected":
    case "failed":
      return "text-danger";
  }
}

/**
 * Which machine is being observed, shown only when that isn't this one.
 *
 * A local session should look exactly as it did before remote existed — an
 * always-on "Local" chip would be noise for the common case. But the moment
 * the files are elsewhere, the feed and the tree are describing another
 * machine, and that has to be on screen.
 */
function ConnectionChip() {
  const info = useConnectionStore((s) => s.info);
  // Only fetched when a stale daemon is on screen — the tooltip names the app
  // version the daemon was expected to match, and nothing else needs it.
  const [appVersion, setAppVersion] = useState<string | null>(null);
  useEffect(() => {
    if (!info.remote || !info.daemonStale) return;
    let cancelled = false;
    getAppInfo()
      .then((a) => {
        if (!cancelled) setAppVersion(a.version);
      })
      .catch(() => {
        // No backend (plain browser) — tooltip still says out of date, just
        // without the expected version number.
      });
    return () => {
      cancelled = true;
    };
  }, [info.remote, info.daemonStale]);

  if (!info.remote) return null;

  const daemonTitle = info.daemonVersion
    ? info.daemonStale
      ? appVersion
        ? `daemon v${info.daemonVersion} (out of date — expected app v${appVersion})`
        : `daemon v${info.daemonVersion} (out of date)`
      : `daemon v${info.daemonVersion}`
    : undefined;

  const suffix = info.state === "connected" ? "" : ` · ${info.state}`;
  return (
    <span
      className={`shrink-0 truncate ${connectionClass(info.state)}`}
      title={info.message ?? daemonTitle}
    >
      {info.label}
      {suffix}
    </span>
  );
}

/**
 * The zoom level, shown only when it isn't 100%.
 *
 * Zoom is easy to nudge by accident and invisible once you've stopped
 * noticing it — "why is everything huge?" needs an answer on screen. At 100%
 * there is nothing to explain, so the chip stays out of the footer.
 */
function ZoomChip() {
  const zoom = useAppearanceStore((s) => s.zoom);
  const resetZoom = useAppearanceStore((s) => s.resetZoom);
  if (zoom === DEFAULT_ZOOM) return null;

  return (
    <button
      type="button"
      onClick={resetZoom}
      title="Reset zoom to 100% (Ctrl+0)"
      className="shrink-0 rounded px-1 tabular-nums hover:bg-hover hover:text-text"
    >
      {Math.round(zoom * 100)}%
    </button>
  );
}

export default function StatusBar() {
  const status = useGitStore((s) => s.status);
  const watcher = useWatcherStore((s) => s.status);
  const capabilities = useGitStore((s) => s.capabilities);
  const sessionTotals = useFeedStore((s) => s.sessionTotals);
  const counts = countsFor(status?.files ?? []);

  let branchLabel = "—";
  if (status) {
    branchLabel = status.isRepository ? (status.branch ?? "no branch") : "not a git repository";
  }

  return (
    <footer className="flex h-7 shrink-0 items-center gap-4 border-t border-border bg-surface-raised px-3 text-[11px] text-text-muted">
      {/* The control replaces the label when mutations are possible; the
          plain label remains for non-repos and read-only fallback. */}
      <BranchControl />
      {!capabilities?.canMutate && <span className="truncate">{branchLabel}</span>}
      {status?.isRepository && (
        <span
          className="flex items-center gap-3 tabular-nums"
          title="Working tree: modified, added, deleted, untracked"
        >
          <span className="text-git-modified">M {counts.modified}</span>
          <span className="text-git-added">A {counts.added}</span>
          <span className="text-git-deleted">D {counts.deleted}</span>
          <span className="text-git-untracked">? {counts.untracked}</span>
        </span>
      )}
      {/* Session running total: file creates/deletes since watch started or Clear.
          Separate from git A/D — those are the tree right now, not the session. */}
      <span
        className="flex items-center gap-3 tabular-nums"
        title="Session total: files created and deleted since watching started (or last Clear)"
      >
        <span className="text-git-added">+ {sessionTotals.created}</span>
        <span className="text-git-deleted">− {sessionTotals.deleted}</span>
      </span>
      <span className="ml-auto flex items-center gap-4 overflow-hidden">
        <ZoomChip />
        <ConnectionChip />
      </span>
      <span
        className={`shrink-0 truncate ${watcher.state === "error" ? "text-danger" : ""}`}
        title={watcher.message ?? undefined}
      >
        {watcherLabel(watcher.state, watcher.message)}
      </span>
    </footer>
  );
}
