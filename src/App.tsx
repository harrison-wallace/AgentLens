import { useEffect } from "react";
import ActivityFeed from "./components/ActivityFeed";
import CommandPalette from "./components/CommandPalette";
import EmptyState from "./components/EmptyState";
import FileTree from "./components/FileTree";
import GitPanel from "./components/GitPanel";
import Preview from "./components/Preview";
import SettingsPanel from "./components/SettingsPanel";
import Splitter from "./components/Splitter";
import StatusBar from "./components/StatusBar";
import WorkspaceHeader from "./components/WorkspaceHeader";
import { onConnection, onFsChanges, onGitStatus, onWatcherStatus } from "./lib/events";
import { useConnectionStore } from "./stores/connectionStore";
import { useFeedStore } from "./stores/feedStore";
import { useGitStore } from "./stores/gitStore";
import { useLayoutStore } from "./stores/layoutStore";
import { usePaletteStore } from "./stores/paletteStore";
import { usePreviewStore } from "./stores/previewStore";
import { useSettingsStore } from "./stores/settingsStore";
import { useTreeStore } from "./stores/treeStore";
import { useWatcherStore } from "./stores/watcherStore";
import { useWorkspaceStore } from "./stores/workspaceStore";

export default function App() {
  const workspace = useWorkspaceStore((s) => s.workspace);
  const restore = useWorkspaceStore((s) => s.restore);
  const loadRecent = useWorkspaceStore((s) => s.loadRecent);
  const selected = useTreeStore((s) => s.selected);
  const selectedIsDir = useTreeStore((s) => s.selectedIsDir);
  const treeWidth = useLayoutStore((s) => s.treeWidth);
  const feedWidth = useLayoutStore((s) => s.feedWidth);
  const treeCollapsed = useLayoutStore((s) => s.treeCollapsed);
  const feedCollapsed = useLayoutStore((s) => s.feedCollapsed);
  const previewCollapsed = useLayoutStore((s) => s.previewCollapsed);
  const setTreeWidth = useLayoutStore((s) => s.setTreeWidth);
  const setFeedWidth = useLayoutStore((s) => s.setFeedWidth);

  useEffect(() => {
    void restore();
    void loadRecent();
    // App-level settings and the connection both outlive the workspace, so
    // they load once here rather than in the per-workspace effect below.
    void useSettingsStore.getState().refreshApp();
    void useConnectionStore.getState().refresh();
  }, [restore, loadRecent]);

  // Subscribed once for the app's lifetime (not per-workspace) so a
  // workspace switch never leaves a stale listener behind.
  useEffect(() => {
    const fsChanges = onFsChanges((events) => {
      useFeedStore.getState().addBatch(events);
      useTreeStore.getState().applyFsChanges(events);

      // The open file may be one of the ones that just changed; re-read it
      // rather than leaving a stale preview on screen.
      const open = usePreviewStore.getState().path;
      if (open && events.some((event) => event.path === open)) {
        void usePreviewStore.getState().refresh();
      }
    });
    const gitStatus = onGitStatus((snapshot) => {
      useGitStore.getState().applySnapshot(snapshot);
    });
    const watcherStatus = onWatcherStatus((status) => {
      useWatcherStore.getState().set(status);
    });
    // A remote link dropping and coming back is the one event that invalidates
    // everything at once: the daemon that comes back is a new process, so what
    // is on screen was read from one that no longer exists.
    const connection = onConnection((info) => {
      const was = useConnectionStore.getState().info.state;
      useConnectionStore.getState().apply(info);

      if (info.state === "disconnected" || info.state === "failed") {
        useFeedStore.getState().beginGap(info.since);
        return;
      }
      // Only a *recovery* invalidates the screen. A first connection is
      // followed by an open, which resets everything anyway — refreshing here
      // would read from a backend that has nothing open yet and flash an
      // error on the way in.
      if (info.state === "connected" && was === "disconnected") {
        useFeedStore.getState().endGap(info.since);
        if (useWorkspaceStore.getState().workspace) {
          void useTreeStore.getState().reloadLoaded();
          void useGitStore.getState().refresh();
          void useWatcherStore.getState().refresh();
        }
      }
    });

    return () => {
      void fsChanges.then((unlisten) => unlisten());
      void gitStatus.then((unlisten) => unlisten());
      void watcherStatus.then((unlisten) => unlisten());
      void connection.then((unlisten) => unlisten());
    };
  }, []);

  // Global chrome keys: `Ctrl+P` file jump, `F11` native fullscreen.
  // Fullscreen is not free in a Tauri window the way it is in a browser tab —
  // the webview never owns the shell, so the app has to call the window API.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "F11") {
        event.preventDefault();
        void (async () => {
          const { getCurrentWindow } = await import("@tauri-apps/api/window");
          const win = getCurrentWindow();
          await win.setFullscreen(!(await win.isFullscreen()));
        })();
        return;
      }
      if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== "p") return;
      // Don't stack the palette on top of another modal, or re-open (and so
      // reset) the one already showing.
      if (useSettingsStore.getState().open || usePaletteStore.getState().open) return;
      event.preventDefault();
      void usePaletteStore.getState().show();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  // A workspace change (open, restore on mount, or close) is the trigger for
  // a fresh tree, git read, feed, and watcher status — the watcher itself is
  // started/stopped backend-side as part of open/close.
  useEffect(() => {
    useTreeStore.getState().reset();
    useGitStore.getState().reset();
    useFeedStore.getState().clear();
    useWatcherStore.getState().reset();
    usePreviewStore.getState().reset();
    usePaletteStore.getState().reset();
    useSettingsStore.getState().reset();
    if (workspace) {
      void useTreeStore.getState().loadDir("");
      void useGitStore.getState().refresh();
      void useGitStore.getState().refreshCapabilities();
      void useGitStore.getState().refreshBranches();
      void useWatcherStore.getState().refresh();
      void useSettingsStore.getState().refresh();
    }
  }, [workspace]);

  // Selecting a directory moves the tree cursor but has nothing to preview,
  // so the pane keeps showing whatever file was open.
  useEffect(() => {
    if (selected && !selectedIsDir) {
      void usePreviewStore.getState().load(selected);
    }
  }, [selected, selectedIsDir]);

  if (!workspace) {
    return <EmptyState />;
  }

  // Exactly one panel absorbs the leftover width. The preview takes it when
  // visible; otherwise it falls to the feed, then the tree — so hiding the
  // middle panel widens what's left instead of leaving a gap.
  const flexPanel = !previewCollapsed ? "preview" : !feedCollapsed ? "feed" : "tree";

  return (
    <div className="flex h-full min-h-0 flex-col bg-surface">
      <WorkspaceHeader />
      <div className="flex min-h-0 flex-1">
        {!treeCollapsed && (
          <>
            <div
              className={`flex min-h-0 min-w-0 flex-col ${
                flexPanel === "tree" ? "flex-1" : "shrink-0"
              }`}
              style={flexPanel === "tree" ? undefined : { width: treeWidth }}
            >
              <div className="min-h-0 flex-1">
                <FileTree />
              </div>
              {/* Source control sits under the tree rather than in its own
                  column: it is about the same files, and a fourth column
                  would cost more width than it earns. */}
              <GitPanel />
            </div>
            {flexPanel !== "tree" && (
              <Splitter width={treeWidth} onResize={setTreeWidth} side="left" label="Resize tree" />
            )}
          </>
        )}
        {!previewCollapsed && (
          <div className="min-h-0 min-w-0 flex-1">
            <Preview />
          </div>
        )}
        {!feedCollapsed && (
          <>
            {flexPanel !== "feed" && (
              <Splitter
                width={feedWidth}
                onResize={setFeedWidth}
                side="right"
                label="Resize feed"
              />
            )}
            <div
              className={`min-h-0 min-w-0 ${flexPanel === "feed" ? "flex-1" : "shrink-0"}`}
              style={flexPanel === "feed" ? undefined : { width: feedWidth }}
            >
              <ActivityFeed />
            </div>
          </>
        )}
      </div>
      <StatusBar />
      <CommandPalette />
      <SettingsPanel />
    </div>
  );
}
