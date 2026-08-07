import { useAppearanceStore } from "../stores/appearanceStore";
import { useBranchPickerStore } from "../stores/branchPickerStore";
import { useBrowseStore } from "../stores/browseStore";
import { useConnectionStore } from "../stores/connectionStore";
import { useGitStore } from "../stores/gitStore";
import { useLayoutStore } from "../stores/layoutStore";
import { usePaletteStore } from "../stores/paletteStore";
import { usePreviewStore } from "../stores/previewStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useShortcutsStore } from "../stores/shortcutsStore";
import { useWorkspaceStore } from "../stores/workspaceStore";

export interface AppCommand {
  id: string;
  title: string;
  /** Display form of the key that runs it, e.g. "Ctrl+P". Documentation only —
   *  App.tsx owns the actual key handling. */
  keys?: string;
  /** Hidden from the palette when false (e.g. git commands with no repo). */
  enabled?: () => boolean;
  run: () => void | Promise<void>;
}

function gitEnabled(): boolean {
  return useGitStore.getState().capabilities?.canMutate === true;
}

/** Flat command registry. Built on each call so `enabled` and store reads are current. */
export function appCommands(): AppCommand[] {
  return [
    // File
    {
      id: "file.openFolder",
      title: "File: Open folder…",
      run: () => {
        const connection = useConnectionStore.getState().info;
        if (connection.remote) {
          void useBrowseStore.getState().start(connection.target);
        } else {
          void useWorkspaceStore.getState().openViaDialog();
        }
      },
    },
    {
      id: "file.goToFile",
      title: "File: Go to file…",
      keys: "Ctrl+P",
      run: () => {
        void usePaletteStore.getState().show();
      },
    },
    {
      id: "file.showCommands",
      title: "File: Show all commands",
      keys: "Ctrl+Shift+P",
      run: () => {
        usePaletteStore.getState().showCommands();
      },
    },
    {
      id: "file.closeTab",
      title: "File: Close tab",
      keys: "Ctrl+W",
      run: () => {
        void usePreviewStore.getState().closeActive();
      },
    },
    {
      id: "file.nextTab",
      title: "File: Next tab",
      keys: "Ctrl+Tab",
      run: () => {
        void usePreviewStore.getState().nextTab();
      },
    },
    {
      id: "file.prevTab",
      title: "File: Previous tab",
      keys: "Ctrl+Shift+Tab",
      run: () => {
        void usePreviewStore.getState().prevTab();
      },
    },

    // View
    {
      id: "view.toggleTree",
      title: "View: Toggle file tree",
      run: () => {
        useLayoutStore.getState().toggleTree();
      },
    },
    {
      id: "view.toggleFeed",
      title: "View: Toggle activity feed",
      run: () => {
        useLayoutStore.getState().toggleFeed();
      },
    },
    {
      id: "view.togglePreview",
      title: "View: Toggle preview",
      run: () => {
        useLayoutStore.getState().togglePreview();
      },
    },
    {
      id: "view.zoomIn",
      title: "View: Zoom in",
      keys: "Ctrl+=",
      run: () => {
        useAppearanceStore.getState().zoomIn();
      },
    },
    {
      id: "view.zoomOut",
      title: "View: Zoom out",
      keys: "Ctrl+-",
      run: () => {
        useAppearanceStore.getState().zoomOut();
      },
    },
    {
      id: "view.resetZoom",
      title: "View: Reset zoom",
      keys: "Ctrl+0",
      run: () => {
        useAppearanceStore.getState().resetZoom();
      },
    },
    {
      id: "view.toggleFullscreen",
      title: "View: Toggle fullscreen",
      keys: "F11",
      run: () => {
        void (async () => {
          const { getCurrentWindow } = await import("@tauri-apps/api/window");
          const win = getCurrentWindow();
          await win.setFullscreen(!(await win.isFullscreen()));
        })();
      },
    },
    {
      id: "view.openSettings",
      title: "View: Open settings",
      run: () => {
        useSettingsStore.getState().setOpen(true);
      },
    },
    {
      id: "view.keyboardShortcuts",
      title: "View: Keyboard shortcuts",
      keys: "F1 / Ctrl+/",
      run: () => {
        useShortcutsStore.getState().show();
      },
    },

    // Git
    {
      id: "git.stageAll",
      title: "Git: Stage all changes",
      enabled: gitEnabled,
      run: () => {
        void useGitStore.getState().stageAll();
      },
    },
    {
      id: "git.unstageAll",
      title: "Git: Unstage all",
      enabled: gitEnabled,
      run: () => {
        void useGitStore.getState().unstageAll();
      },
    },
    {
      id: "git.switchBranch",
      title: "Git: Switch branch…",
      // The picker is rendered by `BranchControl`, which renders nothing until
      // the branch list has loaded — offering the command before then would
      // set an open flag with no widget to honour it.
      enabled: () => gitEnabled() && useGitStore.getState().branches !== null,
      run: () => {
        useBranchPickerStore.getState().show();
      },
    },
    {
      id: "git.stash",
      title: "Git: Stash changes",
      enabled: gitEnabled,
      run: () => {
        void useGitStore.getState().stashPush();
      },
    },
    {
      id: "git.stashPop",
      title: "Git: Pop stash",
      enabled: gitEnabled,
      run: () => {
        void useGitStore.getState().stashPop();
      },
    },
    {
      id: "git.refresh",
      title: "Git: Refresh git status",
      enabled: gitEnabled,
      run: () => {
        void useGitStore.getState().refresh();
      },
    },

    // Session
    {
      id: "session.restart",
      title: "Session: Restart session",
      run: () => {
        void useWorkspaceStore.getState().restartSession();
      },
    },
  ];
}
