import BranchControl from "./BranchControl";
import { useConnectionStore } from "../stores/connectionStore";
import { useGitStore } from "../stores/gitStore";
import { useWatcherStore } from "../stores/watcherStore";
import { countsFor } from "../lib/treeRows";
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
  if (!info.remote) return null;

  const suffix = info.state === "connected" ? "" : ` · ${info.state}`;
  return (
    <span
      className={`shrink-0 truncate ${connectionClass(info.state)}`}
      title={info.message ?? (info.daemonVersion ? `daemon v${info.daemonVersion}` : undefined)}
    >
      {info.label}
      {suffix}
    </span>
  );
}

export default function StatusBar() {
  const status = useGitStore((s) => s.status);
  const watcher = useWatcherStore((s) => s.status);
  const capabilities = useGitStore((s) => s.capabilities);
  const counts = countsFor(status?.files ?? []);

  let branchLabel = "—";
  if (status) {
    branchLabel = status.isRepository ? (status.branch ?? "no branch") : "not a git repository";
  }

  return (
    <footer className="flex h-7 shrink-0 items-center gap-4 border-t border-border bg-surface-raised px-3 text-xs text-text-muted">
      {/* The control replaces the label when mutations are possible; the
          plain label remains for non-repos and read-only fallback. */}
      <BranchControl />
      {!capabilities?.canMutate && <span className="truncate">{branchLabel}</span>}
      {status?.isRepository && (
        <span className="flex items-center gap-3 font-mono">
          <span className="text-git-modified">M {counts.modified}</span>
          <span className="text-git-added">A {counts.added}</span>
          <span className="text-git-deleted">D {counts.deleted}</span>
          <span className="text-git-untracked">? {counts.untracked}</span>
        </span>
      )}
      <span className="ml-auto flex items-center gap-4 overflow-hidden">
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
