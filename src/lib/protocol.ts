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
  /** True when git ignores this entry; only ever set with `showIgnored` on. */
  ignored: boolean;
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

/** Kind of filesystem change reported by the watcher. */
export type FsEventKind = "created" | "modified" | "deleted" | "renamed";

/** One filesystem change, already debounced, filtered, and normalized. */
export interface FsEvent {
  kind: FsEventKind;
  /** Workspace-relative, forward slashes. */
  path: string;
  /** Best-effort; false if the path no longer exists. */
  isDir: boolean;
  /** Unix epoch milliseconds. */
  at: number;
}

/** Lifecycle state of the filesystem watcher for the open workspace. */
export type WatcherState = "off" | "running" | "error";

/** Current watcher state, surfaced in the status bar. */
export interface WatcherStatus {
  state: WatcherState;
  /** Populated on `error`. */
  message: string | null;
}

/**
 * What the preview pane should render for one file. Discriminated on `kind`,
 * mirroring the Rust tagged enum.
 */
export type PreviewPayload =
  | { kind: "text"; path: string; text: string; language: string }
  | { kind: "image"; path: string; mime: string; base64: string }
  | { kind: "binary"; path: string; size: number }
  | { kind: "tooLarge"; path: string; size: number };

/** Why a "diff since session" can't be produced. */
export type DiffUnavailable = "notARepository" | "notTracked";

/** The two sides of a "diff since session" comparison. */
export interface SessionDiff {
  path: string;
  /** Content when the session started; `null` if the file didn't exist. */
  baseline: string | null;
  /** Content now; `null` if the file has since been deleted. */
  current: string | null;
  /** Set when no meaningful comparison exists; null means the diff is usable. */
  unavailable: DiffUnavailable | null;
}

/** Settings that apply to one workspace, persisted per root. */
export interface WorkspaceSettings {
  /** Extra gitignore-syntax globs, hidden from tree, index, and feed. */
  extraIgnores: string[];
  /** Show git-ignored entries in the tree and file index — never the feed. */
  showIgnored: boolean;
}

/**
 * Event names emitted by the backend. Rust `emit` calls and these `listen`
 * calls must both go through the shared constants so the names can't
 * silently drift apart (mirrors `src-tauri/src/protocol.rs`).
 */
export const EVENT_FS_CHANGES = "fs-changes";
export const EVENT_GIT_STATUS = "git-status";
export const EVENT_WATCHER_STATUS = "watcher-status";
