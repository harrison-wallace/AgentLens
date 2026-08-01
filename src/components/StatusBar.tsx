import { useGitStore } from "../stores/gitStore";
import { useWatcherStore } from "../stores/watcherStore";
import { countsFor } from "../lib/treeRows";
import type { WatcherState } from "../lib/protocol";

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

export default function StatusBar() {
  const status = useGitStore((s) => s.status);
  const watcher = useWatcherStore((s) => s.status);
  const counts = countsFor(status?.files ?? []);

  let branchLabel = "—";
  if (status) {
    branchLabel = status.isRepository ? (status.branch ?? "no branch") : "not a git repository";
  }

  return (
    <footer className="flex h-7 shrink-0 items-center gap-4 border-t border-border bg-surface-raised px-3 text-xs text-text-muted">
      <span className="truncate">{branchLabel}</span>
      {status?.isRepository && (
        <span className="flex items-center gap-3 font-mono">
          <span className="text-git-modified">M {counts.modified}</span>
          <span className="text-git-added">A {counts.added}</span>
          <span className="text-git-deleted">D {counts.deleted}</span>
          <span className="text-git-untracked">? {counts.untracked}</span>
        </span>
      )}
      <span
        className={`ml-auto shrink-0 truncate ${watcher.state === "error" ? "text-danger" : ""}`}
        title={watcher.message ?? undefined}
      >
        {watcherLabel(watcher.state, watcher.message)}
      </span>
    </footer>
  );
}
