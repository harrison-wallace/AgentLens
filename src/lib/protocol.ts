/** Mirror of `core/src/protocol.rs`. Keep the two files in sync. */
export interface AppInfo {
  name: string;
  version: string;
}

/**
 * Where the backend runs. `local` is the engine in the app's own process;
 * the others are a daemon reached over `wsl.exe` or `ssh`, because watching
 * files across a network filesystem does not work.
 */
export type ConnectionTarget =
  { kind: "local" } | { kind: "wsl"; distro: string } | { kind: "ssh"; host: string };

/** Liveness of the backend connection. */
export type ConnectionState =
  | "connecting"
  /** Putting a daemon on a remote that hasn't got one. */
  | "installing"
  | "connected"
  | "disconnected"
  | "failed";

/** The current backend connection, surfaced in the status bar. */
export interface ConnectionInfo {
  target: ConnectionTarget;
  state: ConnectionState;
  label: string;
  /** True when the files are not on this machine. */
  remote: boolean;
  /** Why, when the state is `disconnected` or `failed`. */
  message: string | null;
  /** From the handshake, once it has happened. */
  daemonVersion: string | null;
  /**
   * Feature names the daemon reported. Empty means unknown — assume nothing
   * (local, or an older daemon that never sent the field).
   */
  capabilities: string[];
  /**
   * True when a remote daemon's package version differs from this app's.
   * The connection still works; the UI surfaces the fact. Always false local.
   */
  daemonStale: boolean;
  /** Unix epoch ms of the last state change; the feed's gap marker uses it. */
  since: number;
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
  /**
   * True when git ignores this entry. An ignored entry only appears at all
   * with `showIgnored` on, or when a pin / agent context file forces it.
   */
  ignored: boolean;
  /** A file that instructs a coding agent, surfaced as one. */
  agentContext: boolean;
}

/** One entry of the workspace's pinned list, resolved against the disk. */
export interface PinnedEntry {
  /** Workspace-relative, forward slashes — exactly as stored in settings. */
  path: string;
  /** Final path component, for display. */
  name: string;
  isDir: boolean;
  /** False for a pin whose target has been renamed or deleted. */
  exists: boolean;
}

/**
 * One directory offered by the folder picker. Absolute, unlike everything
 * else here, because it is chosen *before* there is a workspace to be
 * relative to.
 */
export interface BrowseEntry {
  name: string;
  /** Absolute path on the backend's machine, forward slashes. */
  path: string;
  /** This directory is a git repository — the most useful thing to know when
   * picking one out of a list of twenty. */
  isRepository: boolean;
}

/** One directory's worth of the folder picker. */
export interface BrowseListing {
  /** Absolute path of the directory that was listed. */
  path: string;
  /** `null` at the filesystem root, which is where "go up" stops. */
  parent: string | null;
  entries: BrowseEntry[];
  /** The directory held more than the listing cap. */
  truncated: boolean;
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

/**
 * Whether git mutations can be offered for the open workspace. Checked on
 * open so the UI degrades to read-only with a hint rather than showing
 * buttons that fail when pressed.
 */
export interface GitCapabilities {
  canMutate: boolean;
  /** `git --version` output, when it ran. */
  version: string | null;
  /** Why mutations are unavailable, for the hint. */
  reason: string | null;
}

/** Local branches and which one is checked out. */
export interface BranchList {
  /** `null` on a detached HEAD or an unborn branch. */
  current: string | null;
  branches: string[];
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

/**
 * An `FsEvent` joined to the agent tool call that most likely caused it.
 * `attribution` is `null` when nothing claimed the change — the user's own
 * editor, or a write the correlation window could not safely tie to a call.
 */
export interface AttributedEvent {
  event: FsEvent;
  /** `null` when nothing claimed it — an external edit. */
  attribution: Attribution | null;
}

/** Which agent tool call is believed to have caused a filesystem change. */
export interface Attribution {
  sessionId: string;
  agent: AgentKind;
  tool: string;
  /** The agent's one-line intent, when the provider supplied one. */
  summary: string | null;
  /**
   * True when claimed by a shell command rather than an explicit path —
   * lower confidence, and the UI says so.
   */
  viaCommand: boolean;
  /** Delegated subagent work. Absent on events from an older daemon. */
  sidechain?: boolean;
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

/** Why a file comparison (session or git) can't be produced. */
export type DiffUnavailable = "notARepository" | "notTracked" | "notText" | "tooLarge";

/** The two sides of a file comparison — session or git. */
export interface SessionDiff {
  path: string;
  /** Left-hand content; `null` if the file didn't exist on that side. */
  baseline: string | null;
  /** Right-hand content; `null` if the file is absent on that side. */
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
  /** Paths kept visible whatever `.gitignore` says, grouped at the tree top. */
  pinned: string[];
}

/** Settings that apply to every workspace, persisted once for the app. */
export interface AppSettings {
  /** Surface `AGENTS.md` and friends whatever `.gitignore` says. */
  showAgentContext: boolean;
  /**
   * Extra directories to search for agent sessions on the connected machine,
   * added to whatever the app detects itself. Persisted per host so a local
   * path is not sent to a remote daemon.
   */
  agentRoots: string[];
  /**
   * What to run on the far side of a WSL or SSH connection. A bare name works
   * when the daemon is on the remote `PATH`; an SSH command runs without a
   * login shell, so an absolute path is often needed.
   */
  daemonCommand: string;
  /**
   * Install the daemon on a remote that hasn't got one, rather than failing
   * with instructions. On by default.
   */
  autoInstallDaemon: boolean;
  /**
   * Max activity-feed batches kept in the UI (oldest drop off). Default 250.
   * Presentation only — the backend ignores this.
   */
  feedMaxEntries: number;
  /**
   * Check GitHub for a newer release at startup. Notify-only — nothing is
   * ever downloaded or installed.
   */
  checkForUpdates: boolean;
  /**
   * OS notification when an agent waits or finishes. Only while unfocused.
   * Presentation only — the backend ignores this.
   */
  notifyAgentState: boolean;
}

/** Result of a notify-only release check. */
export interface UpdateCheck {
  /** The running version. */
  current: string;
  /**
   * Newest published release tag, without the leading `v`; `null` when the
   * check was skipped, failed, or found nothing.
   */
  latest: string | null;
  /** Where to read about it. */
  url: string | null;
  /** True only when `latest` is strictly newer than `current`. */
  newer: boolean;
}

/** Which coding agent a session belongs to. */
export type AgentKind = "claudeCode" | "grok";

/**
 * What an agent is doing right now, normalized across providers.
 * Discriminated on `kind`, mirroring the Rust tagged enum.
 *
 * `detail` on `working` is display-only (e.g. "thinking", a tool name).
 */
export type AgentActivity =
  | { kind: "working"; detail: string | null }
  | { kind: "blocked" }
  | { kind: "idle" }
  | { kind: "stale" };

/** One directory the app searches for agent sessions, as shown in settings. */
export interface AgentRootInfo {
  /** Absolute path, forward slashes — these live outside any workspace. */
  path: string;
  /**
   * Which agent recognises this directory. `null` means none did: for a path
   * the user typed, that is why they see no sessions, so it must be shown.
   */
  agent: AgentKind | null;
  /** Found automatically, as opposed to named by the user. */
  detected: boolean;
  /**
   * Why detection found nothing, when this row is a diagnostic rather than
   * a folder. Empty `path` plus a `note` is the empty-list case.
   */
  note?: string | null;
}

/** A session the app has found on disk but isn't necessarily tailing yet. */
export interface SessionRef {
  /** Provider-assigned id, unique per provider. */
  id: string;
  agent: AgentKind;
  /** Generated session title, when the provider supplies one. */
  title: string | null;
  /** Unix epoch milliseconds of the most recent record seen. */
  lastActivity: number;
  /** What the agent is doing right now, as best the provider can say. */
  activity: AgentActivity;
}

/**
 * One thing an agent did, normalized across providers. Discriminated on
 * `kind`, mirroring the Rust tagged enum.
 */
export type AgentEvent =
  | {
      kind: "sessionStarted";
      sessionId: string;
      agent: AgentKind;
      title: string | null;
      at: number;
    }
  | {
      kind: "toolCall";
      sessionId: string;
      /** Absent on events from an older daemon of this protocol version. */
      agent?: AgentKind;
      at: number;
      tool: string;
      /** One-line description of what the call was for, when derivable. */
      summary: string | null;
      /** Workspace-relative, forward slashes. */
      paths: string[];
      /** The work of a subagent rather than the main thread. */
      sidechain: boolean;
    }
  | {
      kind: "assistantNote";
      sessionId: string;
      agent?: AgentKind;
      at: number;
      text: string;
    }
  | {
      kind: "activityChanged";
      sessionId: string;
      agent?: AgentKind;
      at: number;
      activity: AgentActivity;
    }
  | { kind: "sessionEnded"; sessionId: string; agent?: AgentKind; at: number };

/** The result of tailing a session: what happened, plus what couldn't be read. */
export interface AgentPoll {
  events: AgentEvent[];
  /** Records read since the workspace opened. */
  records: number;
  /** Of those, how many could not be parsed. */
  skipped: number;
}

/**
 * Event names emitted by the backend. Rust `emit` calls and these `listen`
 * calls must both go through the shared constants so the names can't
 * silently drift apart (mirrors `src-tauri/src/protocol.rs`).
 */
export const EVENT_FS_CHANGES = "fs-changes";
export const EVENT_GIT_STATUS = "git-status";
export const EVENT_WATCHER_STATUS = "watcher-status";
/** Filesystem changes with optional agent attribution. */
export const EVENT_ATTRIBUTED = "attributed-changes";
/** Normalized agent activity from the background poller. */
export const EVENT_AGENT_EVENTS = "agent-events";
export const EVENT_CONNECTION = "connection";
