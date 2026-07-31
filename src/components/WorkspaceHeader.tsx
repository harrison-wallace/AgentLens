import { useGitStore } from "../stores/gitStore";
import { useTreeStore } from "../stores/treeStore";
import { useWorkspaceStore } from "../stores/workspaceStore";

export default function WorkspaceHeader() {
  const workspace = useWorkspaceStore((s) => s.workspace);
  const close = useWorkspaceStore((s) => s.close);

  if (!workspace) return null;

  const handleRefresh = () => {
    void useGitStore.getState().refresh();
    void useTreeStore.getState().reloadLoaded();
  };

  return (
    <header className="flex h-10 shrink-0 items-center gap-3 border-b border-border bg-surface-raised px-3">
      <span className="shrink-0 truncate text-sm font-semibold text-text">{workspace.name}</span>
      <span className="min-w-0 flex-1 truncate text-xs text-text-muted" title={workspace.root}>
        {workspace.root}
      </span>
      <div className="flex shrink-0 items-center gap-2">
        <button
          type="button"
          onClick={handleRefresh}
          className="rounded border border-accent px-2 py-1 text-xs font-medium text-accent hover:bg-hover"
        >
          Refresh
        </button>
        <button
          type="button"
          onClick={() => void close()}
          className="rounded border border-border px-2 py-1 text-xs text-text-muted hover:bg-hover hover:text-text"
        >
          Close
        </button>
      </div>
    </header>
  );
}
