import logo from "../assets/lens-logo.svg";
import AgentActivityCluster from "./AgentActivityCluster";
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
  const showIgnored = useSettingsStore((s) => s.settings.showIgnored);
  const toggleShowIgnored = useSettingsStore((s) => s.toggleShowIgnored);

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
    // Diff mode is measured against the session baseline, which just moved —
    // drop every cached preview so each tab re-reads when focused.
    const open = usePreviewStore.getState().tabs.map((t) => t.path);
    usePreviewStore.getState().invalidate(open);
  };

  // `relative` so the agent cluster can centre as an absolute overlay —
  // the path is flex-1 truncate and would push a normal middle child aside.
  return (
    <header className="relative flex h-10 shrink-0 items-center gap-3 border-b border-border bg-surface-raised px-3">
      {/* The only branding in the workspace shell: a mark, not a wordmark —
          the header's job is to say which folder you are watching, and the
          product name is not news to someone already inside the app. */}
      <span className="flex shrink-0 items-center gap-2">
        <img src={logo} alt="AgentLens" title="AgentLens" className="h-4 w-4" />
        <span className="truncate text-xs font-medium text-text">{workspace.name}</span>
      </span>
      <span className="min-w-0 flex-1 truncate text-[11px] text-text-muted" title={workspace.root}>
        {workspace.root}
      </span>
      <AgentActivityCluster />
      <span className="shrink-0 text-[11px] text-text-muted">
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
          className="h-7 rounded border border-border-strong px-2 text-[11px] text-text-muted hover:bg-hover hover:text-text"
        >
          Clear
        </button>
        <button
          type="button"
          onClick={handleRefresh}
          className="h-7 rounded border border-accent px-2 text-[11px] font-medium text-accent hover:bg-hover"
        >
          Refresh
        </button>
        <IconButton
          onClick={() => void toggleShowIgnored()}
          label={showIgnored ? "Hide git-ignored files" : "Show git-ignored files"}
          active={showIgnored}
        >
          ◌
        </IconButton>
        <IconButton onClick={() => openSettings(true)} label="Settings">
          ⚙
        </IconButton>
        <button
          type="button"
          onClick={() => void close()}
          className="h-7 rounded border border-border-strong px-2 text-[11px] text-text-muted hover:bg-hover hover:text-text"
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
      className={`flex h-7 items-center rounded px-2 text-[11px] hover:bg-hover hover:text-text ${
        active === false ? "text-text-muted opacity-50" : "text-text-muted"
      }`}
    >
      {children}
    </button>
  );
}
