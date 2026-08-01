import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  DirEntryNode,
  GitStatusSnapshot,
  PreviewPayload,
  SessionDiff,
  WatcherStatus,
  WorkspaceInfo,
  WorkspaceSettings,
} from "./protocol";

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("get_app_info");
}

export function openWorkspace(path: string): Promise<WorkspaceInfo> {
  return invoke<WorkspaceInfo>("open_workspace", { path });
}

export function closeWorkspace(): Promise<void> {
  return invoke<void>("close_workspace");
}

export function currentWorkspace(): Promise<WorkspaceInfo | null> {
  return invoke<WorkspaceInfo | null>("current_workspace");
}

export function listDir(path: string): Promise<DirEntryNode[]> {
  return invoke<DirEntryNode[]>("list_dir", { path });
}

export function gitStatus(): Promise<GitStatusSnapshot> {
  return invoke<GitStatusSnapshot>("git_status");
}

export function recentWorkspaces(): Promise<string[]> {
  return invoke<string[]>("recent_workspaces");
}

export function watcherStatus(): Promise<WatcherStatus> {
  return invoke<WatcherStatus>("watcher_status");
}

/** Every non-ignored file in the workspace, for the `Ctrl+P` jump. */
export function listFiles(): Promise<string[]> {
  return invoke<string[]>("list_files");
}

export function readPreview(path: string): Promise<PreviewPayload> {
  return invoke<PreviewPayload>("read_preview", { path });
}

/** Hand the file to the OS default application. */
export function openExternally(path: string): Promise<void> {
  return invoke<void>("open_externally", { path });
}

export function sessionDiff(path: string): Promise<SessionDiff> {
  return invoke<SessionDiff>("session_diff", { path });
}

/** Reset "since when" for highlights and diffs, keeping the workspace open. */
export function restartSession(): Promise<WorkspaceInfo> {
  return invoke<WorkspaceInfo>("restart_session");
}

export function workspaceSettings(): Promise<WorkspaceSettings> {
  return invoke<WorkspaceSettings>("workspace_settings");
}

export function setWorkspaceSettings(value: WorkspaceSettings): Promise<WorkspaceSettings> {
  return invoke<WorkspaceSettings>("set_workspace_settings", { value });
}
