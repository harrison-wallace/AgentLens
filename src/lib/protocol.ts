/** Mirror of `src-tauri/src/protocol.rs`. Keep the two files in sync. */
export interface AppInfo {
  name: string;
  version: string;
}

/** The currently open workspace. */
export interface WorkspaceInfo {
  /** Absolute path, forward slashes, for display. */
  root: string;
  /** Final path component. */
  name: string;
  /** Unix epoch milliseconds. */
  watchingSince: number;
}

/** One entry in a single directory listing. */
export interface DirEntryNode {
  name: string;
  /** Workspace-relative, forward slashes. */
  path: string;
  isDir: boolean;
}

/** How git sees one file. */
export type GitStatusKind =
  "added" | "modified" | "deleted" | "renamed" | "untracked" | "conflicted";

/** Git status of a single file. */
export interface GitFileStatus {
  /** Workspace-relative, forward slashes. */
  path: string;
  status: GitStatusKind;
  /** True if the change is in the index. */
  staged: boolean;
}

/** A full git status snapshot for the open workspace. */
export interface GitStatusSnapshot {
  isRepository: boolean;
  /** `null` when detached or unborn. */
  branch: string | null;
  files: GitFileStatus[];
}
