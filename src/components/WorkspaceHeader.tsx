import { useFeedStore } from "../stores/feedStore";
import { useGitStore } from "../stores/gitStore";
import { useLayoutStore } from "../stores/layoutStore";
import { usePreviewStore } from "../stores/previewStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useTreeStore } from "../stores/treeStore";
import { useWorkspaceStore } from "../stores/workspaceStore";

function watchingSinceLabel(at: number): string {
  return new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export default function WorkspaceHeader() {
  const workspace = useWorkspaceStore((s) => s.workspace);
  const close = useWorkspaceStore((s) => s.close);
  const restart = useWorkspaceStore((s) => s.restartSession);
  const treeCollapsed = useLayoutStore((s) => s.treeCollapsed);
  const feedCollapsed = useLayoutStore((s) => s.feedCollapsed);
  const previewCollapsed = useLayoutStore((s) => s.previewCollapsed);
  const toggleTree = useLayoutStore((s) => s.toggleTree);
  const toggleFeed = useLayoutStore((s) => s.toggleFeed);
  const togglePreview = useLayoutStore((s) => s.togglePreview);
  const openSettings = useSettingsStore((s) => s.setOpen);

  if (!workspace) return null;

  const handleRefresh = () => {
    void useGitStore.getState().refresh();
    void useTreeStore.getState().reloadLoaded();
  };

  // Clearing re-baselines the session: highlights, feed, and the diff tab all
  // start measuring from now.
  const handleClear = async () => {
    await restart();
    useFeedStore.getState().clear();
    useTreeStore.getState().clearGlow();
    // The diff tab is measured against the session baseline, which just moved.
    await usePreviewStore.getState().refresh();
  };

  return (
    <header className="flex h-10 shrink-0 items-center gap-3 border-b border-border bg-surface-raised px-3">
      <span className="shrink-0 truncate text-sm font-semibold text-text">{workspace.name}</span>
      <span className="min-w-0 flex-1 truncate text-xs text-text-muted" title={workspace.root}>
        {workspace.root}
      </span>
      <span className="shrink-0 text-xs text-text-muted">
        watching since {watchingSinceLabel(workspace.watchingSince)}
      </span>
      <div className="flex shrink-0 items-center gap-1">
        <IconButton
          onClick={toggleTree}
          label={treeCollapsed ? "Show file tree" : "Hide file tree"}
          active={!treeCollapsed}
        >
          ▤
        </IconButton>
        <IconButton
          onClick={togglePreview}
          label={previewCollapsed ? "Show preview" : "Hide preview"}
          active={!previewCollapsed}
        >
          ▦
        </IconButton>
        <IconButton
          onClick={toggleFeed}
          label={feedCollapsed ? "Show activity feed" : "Hide activity feed"}
          active={!feedCollapsed}
        >
          ▥
        </IconButton>
        <button
          type="button"
          onClick={() => void handleClear()}
          title="Reset what counts as changed since the session started"
          className="rounded border border-border px-2 py-1 text-xs text-text-muted hover:bg-hover hover:text-text"
        >
          Clear
        </button>
        <button
          type="button"
          onClick={handleRefresh}
          className="rounded border border-accent px-2 py-1 text-xs font-medium text-accent hover:bg-hover"
        >
          Refresh
        </button>
        <IconButton onClick={() => openSettings(true)} label="Workspace settings">
          ⚙
        </IconButton>
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

function IconButton({
  onClick,
  label,
  active,
  children,
}: {
  onClick: () => void;
  label: string;
  active?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      className={`rounded px-2 py-1 text-xs hover:bg-hover hover:text-text ${
        active === false ? "text-text-muted opacity-50" : "text-text-muted"
      }`}
    >
      {children}
    </button>
  );
}
