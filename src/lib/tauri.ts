import { invoke } from "@tauri-apps/api/core";
import type {
  AgentPoll,
  AgentRootInfo,
  AppInfo,
  AppSettings,
  BranchList,
  DirEntryNode,
  GitCapabilities,
  GitStatusSnapshot,
  PinnedEntry,
  PreviewPayload,
  SessionDiff,
  SessionRef,
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

/**
 * Git mutations. Each returns the refreshed status, so a caller can update
 * without a second round trip — the backend also broadcasts it, since a
 * mutation the UI doesn't reflect immediately reads as a failure.
 */
export function gitCapabilities(): Promise<GitCapabilities> {
  return invoke<GitCapabilities>("git_capabilities");
}

export function gitStage(paths: string[]): Promise<GitStatusSnapshot> {
  return invoke<GitStatusSnapshot>("git_stage", { paths });
}

export function gitStageAll(): Promise<GitStatusSnapshot> {
  return invoke<GitStatusSnapshot>("git_stage_all");
}

export function gitUnstage(paths: string[]): Promise<GitStatusSnapshot> {
  return invoke<GitStatusSnapshot>("git_unstage", { paths });
}

export function gitUnstageAll(): Promise<GitStatusSnapshot> {
  return invoke<GitStatusSnapshot>("git_unstage_all");
}

export function gitCommit(message: string, amend = false): Promise<GitStatusSnapshot> {
  return invoke<GitStatusSnapshot>("git_commit", { message, amend });
}

export function gitBranches(): Promise<BranchList> {
  return invoke<BranchList>("git_branches");
}

export function gitSwitchBranch(name: string): Promise<GitStatusSnapshot> {
  return invoke<GitStatusSnapshot>("git_switch_branch", { name });
}

export function gitCreateBranch(name: string): Promise<GitStatusSnapshot> {
  return invoke<GitStatusSnapshot>("git_create_branch", { name });
}

export function gitStashPush(message?: string): Promise<GitStatusSnapshot> {
  return invoke<GitStatusSnapshot>("git_stash_push", { message: message ?? null });
}

export function gitStashPop(): Promise<GitStatusSnapshot> {
  return invoke<GitStatusSnapshot>("git_stash_pop");
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

/** App-level settings; unlike the workspace ones, no workspace need be open. */
export function appSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("app_settings");
}

export function setAppSettings(value: AppSettings): Promise<AppSettings> {
  return invoke<AppSettings>("set_app_settings", { value });
}

/** The pinned paths resolved against the disk, for the tree's Pinned group. */
export function pinnedEntries(): Promise<PinnedEntry[]> {
  return invoke<PinnedEntry[]>("pinned_entries");
}

/** Agent sessions for the open workspace, most recently active first. */
export function agentSessions(): Promise<SessionRef[]> {
  return invoke<SessionRef[]>("agent_sessions");
}

/** Where the app looks for agent sessions, and where each entry came from. */
export function agentRoots(): Promise<AgentRootInfo[]> {
  return invoke<AgentRootInfo[]>("agent_roots");
}

/** Records appended to `session` since the last call — new activity only. */
export function agentEvents(session: SessionRef): Promise<AgentPoll> {
  return invoke<AgentPoll>("agent_events", { session });
}
