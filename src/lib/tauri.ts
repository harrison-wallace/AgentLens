import { invoke } from "@tauri-apps/api/core";
import type { DirEntryNode, GitStatusSnapshot, WorkspaceInfo, AppInfo } from "./protocol";

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
