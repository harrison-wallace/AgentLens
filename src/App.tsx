import { useEffect } from "react";
import ActivityFeed from "./components/ActivityFeed";
import CommandPalette from "./components/CommandPalette";
import EmptyState from "./components/EmptyState";
import FileTree from "./components/FileTree";
import GitPanel from "./components/GitPanel";
import Preview from "./components/Preview";
import SettingsPanel from "./components/SettingsPanel";
import ShortcutsHelp from "./components/ShortcutsHelp";
import Splitter from "./components/Splitter";
import StatusBar from "./components/StatusBar";
import Toasts from "./components/Toasts";
import WorkspaceHeader from "./components/WorkspaceHeader";
import {
  onAgentEvents,
  onAttributedChanges,
  onConnection,
  onFsChanges,
  onGitStatus,
  onWatcherStatus,
} from "./lib/events";
import { formatLocation } from "./lib/location";
import { quickPickOpen } from "./lib/quickPick";
import { checkForUpdate } from "./lib/tauri";
import { useAgentStore } from "./stores/agentStore";
import { useAppearanceStore } from "./stores/appearanceStore";
import { useConnectionStore } from "./stores/connectionStore";
import { useFeedStore } from "./stores/feedStore";
import { useGitStore } from "./stores/gitStore";
import { useLayoutStore } from "./stores/layoutStore";
import { usePaletteStore } from "./stores/paletteStore";
import { usePreviewStore } from "./stores/previewStore";
import { useSettingsStore } from "./stores/settingsStore";
import { useShortcutsStore } from "./stores/shortcutsStore";
import { useToastStore } from "./stores/toastStore";
import { useTreeStore } from "./stores/treeStore";
import { useWatcherStore } from "./stores/watcherStore";
import { useWorkspaceStore } from "./stores/workspaceStore";

/** True when the event target is a field the user is typing in — chrome
 * shortcuts like Ctrl+W must not steal those keystrokes. */
function isEditableKeyTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag === "INPUT") {
    const type = (target as HTMLInputElement).type;
    // Buttons and similar are not typing surfaces.
    return !["button", "checkbox", "radio", "submit", "reset", "file", "range", "color"].includes(
      type,
    );
  }
  return target.closest("textarea, select, input:not([type=button]):not([type=checkbox])") !== null;
}

export default function App() {
  const workspace = useWorkspaceStore((s) => s.workspace);
  const restore = useWorkspaceStore((s) => s.restore);
  const loadRecent = useWorkspaceStore((s) => s.loadRecent);
  const connectionTarget = useConnectionStore((s) => s.info.target);
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
    void useSettingsStore
      .getState()
      .refreshApp()
      .then(() => {
        // Notify-only: fire once after settings land, and only when enabled.
        // A failure or "already current" is silence — never a toast about a
        // machine that is simply offline.
        if (!useSettingsStore.getState().app.checkForUpdates) return;
        void checkForUpdate()
          .then((result) => {
            if (!result.newer || !result.latest) return;
            useToastStore
              .getState()
              .push(
                `AgentLens ${result.latest} is available`,
                undefined,
                result.url ? { href: result.url, label: "View release" } : undefined,
              );
          })
          .catch(() => {
            // The command itself returns a quiet non-event on failure; this
            // only covers invoke being unavailable (plain browser, tests).
          });
      });
    void useConnectionStore.getState().refresh();
    // The webview starts every launch at 100%, so a stored zoom has to be
    // re-applied rather than merely read.
    useAppearanceStore.getState().apply();
  }, [restore, loadRecent]);

  // Subscribed once for the app's lifetime (not per-workspace) so a
  // workspace switch never leaves a stale listener behind.
  useEffect(() => {
    const fsChanges = onFsChanges((events) => {
      useFeedStore.getState().addBatch(events);
      useTreeStore.getState().applyFsChanges(events);

      // Drop cached previews for paths that changed; re-read the active tab
      // when it is among them so the pane does not go stale.
      const paths = events.map((event) => event.path);
      usePreviewStore.getState().invalidate(paths);
    });
    const gitStatus = onGitStatus((snapshot) => {
      useGitStore.getState().applySnapshot(snapshot);
    });
    const watcherStatus = onWatcherStatus((status) => {
      useWatcherStore.getState().set(status);
    });
    // Agent poller batches and the correlator's late attributions. Same
    // unlisten-on-promise pattern as the other listeners: if this effect
    // cleans up before listen resolves, the subscription is already live
    // and dropping the handle would leak it.
    const agentEvents = onAgentEvents((events) => {
      useAgentStore.getState().apply(events);
    });
    const attributed = onAttributedChanges((events) => {
      useFeedStore.getState().applyAttribution(events);
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
          void useAgentStore.getState().refresh();
        }
      }
    });

    return () => {
      void fsChanges.then((unlisten) => unlisten());
      void gitStatus.then((unlisten) => unlisten());
      void watcherStatus.then((unlisten) => unlisten());
      void agentEvents.then((unlisten) => unlisten());
      void attributed.then((unlisten) => unlisten());
      void connection.then((unlisten) => unlisten());
    };
  }, []);

  // Global chrome keys: `Ctrl+P` file jump, `Ctrl+Shift+P` command palette,
  // `F1` / `Ctrl+/` shortcuts help, `F11` native fullscreen, `Ctrl +/-/0`
  // zoom, `Ctrl+W` close tab, `Ctrl+Tab` cycle tabs.
  // Fullscreen is not free in a Tauri window the way it is in a browser tab —
  // the webview never owns the shell, so the app has to call the window API.
  // Zoom is handled here rather than by Tauri's `zoomHotkeysEnabled` polyfill,
  // which has no reset, no persistence, and would fight this handler for the
  // same keys.
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
      if (event.key === "F1") {
        event.preventDefault();
        useShortcutsStore.getState().show();
        return;
      }
      if (!(event.ctrlKey || event.metaKey)) return;

      // `+` and `_` are the shifted forms of the same two keys, and both
      // reach here from a keyboard where the unshifted press doesn't.
      const appearance = useAppearanceStore.getState();
      if (event.key === "=" || event.key === "+") {
        event.preventDefault();
        appearance.zoomIn();
        return;
      }
      if (event.key === "-" || event.key === "_") {
        event.preventDefault();
        appearance.zoomOut();
        return;
      }
      if (event.key === "0") {
        event.preventDefault();
        appearance.resetZoom();
        return;
      }

      // Tab chrome: stay out of text fields (commit box, settings, etc.).
      if (event.key === "Tab") {
        if (isEditableKeyTarget(event.target)) return;
        event.preventDefault();
        if (event.shiftKey) void usePreviewStore.getState().prevTab();
        else void usePreviewStore.getState().nextTab();
        return;
      }

      if (event.key.toLowerCase() === "w") {
        if (isEditableKeyTarget(event.target)) return;
        event.preventDefault();
        void usePreviewStore.getState().closeActive();
        return;
      }

      // Shortcuts help: `Ctrl+/` (and F1 above). Guarded like the other chrome
      // keys — `Ctrl+/` is a comment shortcut elsewhere, and muscle memory in
      // the commit box must not pop an overlay.
      if (event.key === "/" || event.key === "?") {
        if (isEditableKeyTarget(event.target)) return;
        event.preventDefault();
        useShortcutsStore.getState().show();
        return;
      }

      if (event.key.toLowerCase() !== "p") return;
      // Don't stack the palette on top of another modal, or re-open (and so
      // reset) the one already showing — including the branch picker, which
      // is the same widget.
      if (useSettingsStore.getState().open || quickPickOpen()) return;
      event.preventDefault();
      // Shift distinguishes the command palette from the file jump.
      if (event.shiftKey) usePaletteStore.getState().showCommands();
      else void usePaletteStore.getState().show();
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
    useAgentStore.getState().reset();
    usePaletteStore.getState().reset();
    useSettingsStore.getState().reset();
    if (workspace) {
      void useTreeStore.getState().loadDir("");
      void useGitStore.getState().refresh();
      void useGitStore.getState().refreshCapabilities();
      void useGitStore.getState().refreshBranches();
      void useWatcherStore.getState().refresh();
      void useSettingsStore.getState().refresh();
      void useAgentStore.getState().refresh();
    }
  }, [workspace]);

  // Open tabs are keyed by full location (scheme + host/distro + root), not
  // root alone — `/home/h/proj` on two SSH hosts must not share a tab set.
  // Separate from the workspace effect so a connection refresh after restore
  // can re-bind without wiping the tree.
  const tabsLocationKey = workspace ? formatLocation(connectionTarget, workspace.root) : null;
  useEffect(() => {
    if (tabsLocationKey) {
      void usePreviewStore.getState().bindWorkspace(tabsLocationKey);
    } else {
      usePreviewStore.getState().reset();
    }
  }, [tabsLocationKey]);

  if (!workspace) {
    return (
      <>
        <EmptyState />
        <Toasts />
      </>
    );
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
      <ShortcutsHelp />
      <Toasts />
    </div>
  );
}
