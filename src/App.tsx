import { useEffect } from "react";
import EmptyState from "./components/EmptyState";
import FileTree from "./components/FileTree";
import StatusBar from "./components/StatusBar";
import WorkspaceHeader from "./components/WorkspaceHeader";
import { useGitStore } from "./stores/gitStore";
import { useTreeStore } from "./stores/treeStore";
import { useWorkspaceStore } from "./stores/workspaceStore";

export default function App() {
  const workspace = useWorkspaceStore((s) => s.workspace);
  const restore = useWorkspaceStore((s) => s.restore);
  const loadRecent = useWorkspaceStore((s) => s.loadRecent);
  const selected = useTreeStore((s) => s.selected);

  useEffect(() => {
    void restore();
    void loadRecent();
  }, [restore, loadRecent]);

  // The watcher lands in a later slice, so a workspace change (open, restore
  // on mount, or close) is the only trigger for a fresh tree + git read.
  useEffect(() => {
    useTreeStore.getState().reset();
    useGitStore.getState().reset();
    if (workspace) {
      void useTreeStore.getState().loadDir("");
      void useGitStore.getState().refresh();
    }
  }, [workspace]);

  if (!workspace) {
    return <EmptyState />;
  }

  return (
    <div className="flex h-full min-h-0 flex-col bg-surface">
      <WorkspaceHeader />
      <div className="flex min-h-0 flex-1">
        <div className="min-h-0 w-80 shrink-0 border-r border-border">
          <FileTree />
        </div>
        <div className="flex min-h-0 flex-1 items-center justify-center p-4 text-center text-sm text-text-muted">
          {selected ? (
            <p>
              <span className="text-text">{selected}</span> — preview lands in a later slice.
            </p>
          ) : (
            <p>Select a file to preview it in a later slice.</p>
          )}
        </div>
      </div>
      <StatusBar />
    </div>
  );
}
