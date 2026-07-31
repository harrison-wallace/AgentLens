import { useGitStore } from "../stores/gitStore";
import { countsFor } from "../lib/treeRows";

export default function StatusBar() {
  const status = useGitStore((s) => s.status);
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
      <span className="ml-auto shrink-0">watcher: off</span>
    </footer>
  );
}
