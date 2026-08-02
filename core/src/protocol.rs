//! Serializable types crossing the UI <-> backend boundary.
//!
//! Everything a front end sends or receives is defined here and mirrored in
//! `src/lib/protocol.ts`. This is the seam the crate split turns on: today
//! these types are passed in-process to a Tauri command, and a later phase
//! sends the same bytes over a pipe to a daemon running where the files are.
//! Nothing here may reference a transport, a window, or Tauri — if it did,
//! that move would stop being possible.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Version of the command/event vocabulary below.
///
/// The policy, written out in `docs/PROTOCOL.md`: within one major version
/// changes are **additive only** — new commands, new optional fields, new
/// event names. Anything that removes or repurposes an existing shape bumps
/// this number, and `Hello` is what makes the mismatch a clear error instead
/// of a puzzling one. Every type in this file carries `#[serde(default)]` or
/// an `Option` where a future version might not send it, for the same reason.
pub const PROTOCOL_VERSION: u32 = 1;

/// Identity of the running application, surfaced in the window title and UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

/// A backend's answer to the handshake: who it is and what it speaks.
///
/// Sent before anything else on a connection, because everything after it
/// depends on both ends agreeing about the vocabulary.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
// `default` so a `Hello` from a future backend that grew a field still parses
// far enough for the version check to produce the guided error.
#[serde(rename_all = "camelCase", default)]
pub struct Hello {
    pub name: String,
    /// The backend's package version — not necessarily the UI's.
    pub version: String,
    pub protocol_version: u32,
    /// Optional features this backend has. Empty is a valid answer; the UI
    /// must not require anything here to function.
    pub capabilities: Vec<String>,
}

/// The result type for every operation a front end can invoke, so the error
/// shape is uniform whatever the transport.
pub type CommandResult<T> = Result<T, String>;

/// The currently open workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    /// Absolute path, forward slashes, for display.
    pub root: String,
    /// Final path component.
    pub name: String,
    /// Unix epoch milliseconds.
    pub watching_since: i64,
}

/// One entry in a single directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryNode {
    pub name: String,
    /// Workspace-relative, forward slashes.
    pub path: String,
    pub is_dir: bool,
    /// True when git ignores this entry. An ignored entry only appears at all
    /// when `show_ignored` is on or something forces it visible (a pin, or an
    /// agent context file).
    pub ignored: bool,
    /// This is a file that instructs a coding agent, and the app is surfacing
    /// it as one. False when the app-level setting is off.
    pub agent_context: bool,
}

/// One entry of the workspace's pinned list, resolved against the disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinnedEntry {
    /// Workspace-relative, forward slashes — exactly as stored in settings.
    pub path: String,
    /// Final path component, for display.
    pub name: String,
    pub is_dir: bool,
    /// False for a pin whose target has been renamed or deleted. Dead pins are
    /// shown rather than dropped: a pin vanishing on its own is worse than a
    /// visible stale one.
    pub exists: bool,
}

/// One directory offered by the folder picker.
///
/// Absolute, unlike everything else in this file, because it is chosen
/// *before* there is a workspace for a path to be relative to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseEntry {
    pub name: String,
    /// Absolute path on the backend's machine, forward slashes.
    pub path: String,
    /// This directory is a git repository. Not a filter — plenty of useful
    /// workspaces aren't repositories — but it is the single most useful thing
    /// to know when picking one out of a list of twenty.
    pub is_repository: bool,
}

/// One directory's worth of the folder picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseListing {
    /// Absolute path of the directory that was listed.
    pub path: String,
    /// `None` at the filesystem root, which is where "go up" stops.
    pub parent: Option<String>,
    pub entries: Vec<BrowseEntry>,
    /// The directory held more than the listing cap. Shown rather than hidden,
    /// so a picker that seems to be missing something says why.
    pub truncated: bool,
}

/// How git sees one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitStatusKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

/// Git status of a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFileStatus {
    /// Workspace-relative, forward slashes.
    pub path: String,
    pub status: GitStatusKind,
    /// True if the change is in the index.
    pub staged: bool,
}

/// A full git status snapshot for the open workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusSnapshot {
    pub is_repository: bool,
    /// `None` when detached or unborn.
    pub branch: Option<String>,
    pub files: Vec<GitFileStatus>,
}

/// Whether git mutations can be offered for the open workspace.
///
/// Checked when a workspace opens so the UI can degrade to read-only with a
/// hint rather than showing buttons that fail the moment they're pressed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCapabilities {
    pub can_mutate: bool,
    /// `git --version` output, when it ran.
    pub version: Option<String>,
    /// Why mutations are unavailable, for the hint.
    pub reason: Option<String>,
}

/// Local branches and which one is checked out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchList {
    /// `None` on a detached HEAD or an unborn branch.
    pub current: Option<String>,
    pub branches: Vec<String>,
}

/// Kind of filesystem change reported by the watcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FsEventKind {
    Created,
    Modified,
    Deleted,
    Renamed,
}

/// One filesystem change, already debounced, filtered, and normalized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsEvent {
    pub kind: FsEventKind,
    /// Workspace-relative, forward slashes.
    pub path: String,
    /// Best-effort; false if the path no longer exists.
    pub is_dir: bool,
    /// Unix epoch milliseconds.
    pub at: i64,
}

/// Lifecycle state of the filesystem watcher for the open workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WatcherState {
    Off,
    Running,
    Error,
}

/// Current watcher state, surfaced in the status bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatcherStatus {
    pub state: WatcherState,
    /// Populated on `Error`.
    pub message: Option<String>,
}

/// What the preview pane should render for one file. Tagged by `kind` so the
/// TS mirror is a discriminated union.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PreviewPayload {
    /// Decoded UTF-8 text, with a Shiki language id inferred from the
    /// extension (`"text"` when nothing matches).
    Text {
        path: String,
        text: String,
        language: String,
    },
    /// Base64-encoded image bytes; the UI builds a `data:` URL.
    Image {
        path: String,
        mime: String,
        base64: String,
    },
    /// Not decodable as text, so there is nothing read-only to show.
    Binary { path: String, size: u64 },
    /// Over the preview size cap.
    TooLarge { path: String, size: u64 },
}

/// Why a "diff since session" can't be produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffUnavailable {
    /// The workspace isn't a git repository, and the baseline comes from git.
    NotARepository,
    /// Git ignores this file, so it has no `HEAD` blob and never appears in
    /// the status output the session baseline is captured from. Showing it as
    /// wholly added would misrepresent a file that may well have existed.
    NotTracked,
}

/// The two sides of a "diff since session" comparison. The line diff itself
/// is computed in the UI, which already has a diff library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDiff {
    pub path: String,
    /// Content when the session started; `None` if the file didn't exist.
    pub baseline: Option<String>,
    /// Content now; `None` if the file has since been deleted.
    pub current: Option<String>,
    /// Set when no meaningful comparison exists; `None` means the diff is
    /// usable.
    pub unavailable: Option<DiffUnavailable>,
}

/// Settings that apply to one workspace, persisted per root.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
// `default` so settings persisted by an older version still deserialize
// rather than resetting the whole workspace entry.
#[serde(rename_all = "camelCase", default)]
pub struct WorkspaceSettings {
    /// Extra gitignore-syntax globs, hidden from the tree, the file index,
    /// and the activity feed.
    pub extra_ignores: Vec<String>,
    /// Show entries git ignores in the tree and file index. Deliberately does
    /// not affect the activity feed: hiding ignored churn there is what keeps
    /// an `npm install` from flooding it.
    pub show_ignored: bool,
    /// Workspace-relative paths (files and directories) kept visible whatever
    /// `.gitignore` says, and grouped at the top of the tree.
    pub pinned: Vec<String>,
}

/// Settings that apply to every workspace, persisted once for the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// `default` for the same forward-compatibility reason as `WorkspaceSettings`,
// and because the defaults here are not all `false`.
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// Surface the files that instruct a coding agent (`AGENTS.md` and
    /// friends) whatever `.gitignore` says. App-level because "always show me
    /// `AGENTS.md`" is how you work, not a property of one repo.
    pub show_agent_context: bool,
    /// Extra directories to search for agent sessions, added to whatever the
    /// app detects by itself.
    ///
    /// Needed because detection can only ever be a guess: an agent's storage
    /// location is a convention its authors never promised, and where a user
    /// keeps multiple profiles is a convention on top of that. Anyone whose
    /// layout we can't guess has no other way to make the feature work.
    pub agent_roots: Vec<String>,
    /// What to run on the far side of a WSL or SSH connection.
    ///
    /// A bare name works when the daemon is on the remote `PATH` — but an SSH
    /// command runs without a login shell, so `~/.local/bin` typically is not
    /// on it. An absolute path here is the fix, and it is a setting rather
    /// than a guess because only the user knows where they put it.
    ///
    /// Leaving this at the default is what enables the find-or-install
    /// bootstrap; naming a command here is an instruction to run exactly that.
    pub daemon_command: String,
    /// Install the daemon on a remote that hasn't got one, instead of failing
    /// with instructions.
    ///
    /// On by default, because the alternative is asking someone to hand-place
    /// a binary before the app will talk to a machine — and because the app
    /// already has permission to run commands there, which is strictly more
    /// than writing one file into the user's own home directory.
    pub auto_install_daemon: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            show_agent_context: true,
            agent_roots: Vec::new(),
            daemon_command: "agentlens-daemon".to_string(),
            auto_install_daemon: true,
        }
    }
}

/// Which coding agent a session belongs to. Providers are added behind the
/// `AgentProvider` trait, so this grows without anything downstream changing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentKind {
    ClaudeCode,
    Opencode,
}

/// One directory the app searches for agent sessions, as shown in settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRootInfo {
    /// Absolute path, forward slashes. Not workspace-relative — these live
    /// outside any workspace, which is the whole point of them.
    pub path: String,
    /// Which agent recognises this directory. `None` means none did: for a
    /// path the user typed, that is the explanation for why they see no
    /// sessions, so the UI has to say so rather than skip it quietly.
    pub agent: Option<AgentKind>,
    /// Found automatically, as opposed to named by the user.
    pub detected: bool,
}

/// A session the app has found on disk but is not necessarily tailing yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    /// Provider-assigned id, unique per provider.
    pub id: String,
    pub agent: AgentKind,
    /// Generated session title, when the provider supplies one.
    pub title: Option<String>,
    /// Unix epoch milliseconds of the most recent record seen.
    pub last_activity: i64,
}

/// One thing an agent did, normalized across providers. Everything
/// downstream — correlation, the feed, the session panel — consumes only
/// this, so adding a provider touches nothing outside `agents/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AgentEvent {
    SessionStarted {
        session_id: String,
        agent: AgentKind,
        title: Option<String>,
        at: i64,
    },
    /// A single tool invocation. `paths` are workspace-relative with forward
    /// slashes like every other path in the protocol, and exclude anything
    /// the tool touched outside the workspace.
    ToolCall {
        session_id: String,
        at: i64,
        tool: String,
        /// One-line description of what the call was for, when derivable.
        summary: Option<String>,
        paths: Vec<String>,
        /// The work of a subagent rather than the main thread.
        sidechain: bool,
    },
    /// The instruction the session is currently working on — the "why" behind
    /// the tool calls around it.
    AssistantNote {
        session_id: String,
        at: i64,
        text: String,
    },
    SessionEnded {
        session_id: String,
        at: i64,
    },
}

/// The result of tailing a session: what happened, plus how much of the
/// transcript the app failed to understand. The count is surfaced quietly in
/// the UI rather than as an error — these formats drift without warning, and
/// the app still works when it can only read some of a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPoll {
    pub events: Vec<AgentEvent>,
    /// Records read since the workspace opened.
    pub records: u64,
    /// Of those, how many could not be parsed.
    pub skipped: u64,
}

/// Everything a front end can ask a backend to do.
///
/// One enum rather than one function per operation, because the whole point
/// of the split is that these travel down a pipe. A backend running in-process
/// matches on this; a backend running inside a WSL distro receives the same
/// value as a line of JSON.
///
/// Getters that would collide with the type they return are prefixed `Get`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum Command {
    /// Handshake. First command on any connection; see [`Hello`].
    ///
    /// `rename_all` on an enum renames its *variants*, not their fields, so
    /// this one goes over the wire as `protocol_version` while every other
    /// protocol type is camelCase. Unifying that would break every daemon
    /// already installed, for no user-visible gain — so the spelling stays,
    /// and the alias makes accepting the tidier one a non-breaking change
    /// whenever a protocol bump happens for a better reason.
    Hello {
        #[serde(alias = "protocolVersion")]
        protocol_version: u32,
    },
    /// Liveness probe. Cheap, touches no state.
    Ping,

    /// Open `path` and capture session baselines. Does **not** start watching
    /// — the caller follows with `SetWorkspaceSettings`, since only it knows
    /// what the persisted settings for the (now canonical) root are.
    OpenWorkspace {
        path: String,
    },
    CloseWorkspace,
    CurrentWorkspace,
    RestartSession,
    GetWatcherStatus,

    ListDir {
        path: String,
    },
    ListFiles,
    /// Directories inside an *absolute* path on the backend's machine, for
    /// choosing a workspace. `None` starts at the home directory. Needs no
    /// workspace open — it is what happens before there is one.
    BrowseDir {
        path: Option<String>,
    },
    PinnedEntries,
    ReadPreview {
        path: String,
    },
    /// The absolute path on the backend's machine for `path`, validated to be
    /// inside the workspace. Handing it to a viewer is the caller's job —
    /// which application to use, and how to reach a remote filesystem from
    /// this side of the connection, are both local questions.
    ResolveForOpen {
        path: String,
    },
    SessionDiff {
        path: String,
    },

    GitStatus,
    GitCapabilities,
    GitStage {
        paths: Vec<String>,
    },
    GitStageAll,
    GitUnstage {
        paths: Vec<String>,
    },
    GitUnstageAll,
    GitCommit {
        message: String,
        amend: bool,
    },
    GitBranches,
    GitSwitchBranch {
        name: String,
    },
    GitCreateBranch {
        name: String,
    },
    GitStashPush {
        message: Option<String>,
    },
    GitStashPop,

    GetWorkspaceSettings,
    /// Put workspace settings into effect and (re)start the watcher against
    /// them. Persisting is the caller's business.
    SetWorkspaceSettings {
        value: WorkspaceSettings,
    },
    GetAppSettings,
    SetAppSettings {
        value: AppSettings,
    },

    AgentSessions,
    AgentRoots,
    AgentEvents {
        session: SessionRef,
    },
}

/// One line on the wire between a front end and a remote backend.
///
/// Requests carry an id so responses can be matched out of order, and events
/// carry none because nothing is waiting on them. Serialized one per line to
/// stdout; stderr on that channel is logs only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Frame {
    Request {
        id: u64,
        command: Command,
    },
    /// Exactly one of `result`/`error` is present. A `null` result is a
    /// perfectly good success — commands returning nothing send it.
    Response {
        id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// A push from the backend, named by one of the `EVENT_*` constants.
    Event {
        event: String,
        payload: Value,
    },
}

/// Where a backend runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConnectionTarget {
    /// In-process, watching this machine's filesystem.
    Local,
    /// A WSL distro, reached by `wsl.exe -d <distro>`.
    Wsl { distro: String },
    /// A host from the user's ssh config, reached by the system `ssh` binary
    /// so that auth, agents, jump hosts and 2FA behave exactly as they do in
    /// their terminal.
    Ssh { host: String },
}

impl ConnectionTarget {
    /// True when the files are not on this machine — which is what decides
    /// whether local-only affordances (opening a file in another app) apply.
    pub fn is_remote(&self) -> bool {
        !matches!(self, ConnectionTarget::Local)
    }

    /// Short label for the status bar.
    pub fn label(&self) -> String {
        match self {
            ConnectionTarget::Local => "Local".to_string(),
            ConnectionTarget::Wsl { distro } => format!("WSL: {distro}"),
            ConnectionTarget::Ssh { host } => format!("SSH: {host}"),
        }
    }
}

/// Liveness of the current backend connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionState {
    Connecting,
    /// Putting a daemon on a remote that hasn't got one. Its own state because
    /// it is the one step that takes long enough to look like a hang.
    Installing,
    Connected,
    /// The daemon died or the transport failed. A reconnect is in flight.
    Disconnected,
    /// Reconnecting was given up on, or the handshake was refused. Needs the
    /// user to do something.
    Failed,
}

/// The current backend connection, surfaced in the status bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConnectionInfo {
    pub target: ConnectionTarget,
    pub state: ConnectionState,
    pub label: String,
    pub remote: bool,
    /// Why, when the state is `Disconnected` or `Failed`.
    pub message: Option<String>,
    /// From the handshake, once it has happened.
    pub daemon_version: Option<String>,
    /// Unix epoch milliseconds of the last state change. The feed uses this
    /// to place a gap marker over the window it missed.
    pub since: i64,
}

impl Default for ConnectionInfo {
    fn default() -> Self {
        ConnectionInfo {
            target: ConnectionTarget::Local,
            state: ConnectionState::Connected,
            label: "Local".to_string(),
            remote: false,
            message: None,
            daemon_version: None,
            since: 0,
        }
    }
}

/// Event names emitted by the backend. Rust `emit` calls and TS `listen`
/// calls must both go through these constants (mirrored in
/// `src/lib/protocol.ts`) so the names can't silently drift apart.
pub const EVENT_FS_CHANGES: &str = "fs-changes";
pub const EVENT_GIT_STATUS: &str = "git-status";
pub const EVENT_WATCHER_STATUS: &str = "watcher-status";
/// Connection lifecycle. Only ever non-trivial for a remote backend, but it
/// is emitted for local too so the UI has one code path.
pub const EVENT_CONNECTION: &str = "connection";
/// Periodic no-op from a remote daemon, to keep an idle SSH link from being
/// reaped by whatever NAT or firewall sits in the middle. Consumed by the
/// transport; never forwarded to the front end.
pub const EVENT_HEARTBEAT: &str = "heartbeat";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_serializes_with_camel_case_fields() {
        let info = AppInfo {
            name: "AgentLens".into(),
            version: "0.0.1".into(),
        };

        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["name"], "AgentLens");
        assert_eq!(json["version"], "0.0.1");
    }

    #[test]
    fn fs_event_kind_round_trips_through_camel_case_json() {
        let cases = [
            (FsEventKind::Created, "\"created\""),
            (FsEventKind::Modified, "\"modified\""),
            (FsEventKind::Deleted, "\"deleted\""),
            (FsEventKind::Renamed, "\"renamed\""),
        ];
        for (kind, expected) in cases {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, expected);
            let round_tripped: FsEventKind = serde_json::from_str(&json).unwrap();
            assert_eq!(round_tripped, kind);
        }
    }

    #[test]
    fn watcher_state_round_trips_through_camel_case_json() {
        let cases = [
            (WatcherState::Off, "\"off\""),
            (WatcherState::Running, "\"running\""),
            (WatcherState::Error, "\"error\""),
        ];
        for (state, expected) in cases {
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(json, expected);
            let round_tripped: WatcherState = serde_json::from_str(&json).unwrap();
            assert_eq!(round_tripped, state);
        }
    }

    #[test]
    fn fs_event_serializes_with_camel_case_fields() {
        let event = FsEvent {
            kind: FsEventKind::Modified,
            path: "src/main.rs".into(),
            is_dir: false,
            at: 1_700_000_000_000,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "modified");
        assert_eq!(json["path"], "src/main.rs");
        assert_eq!(json["isDir"], false);
        assert_eq!(json["at"], 1_700_000_000_000i64);
    }

    #[test]
    fn commands_are_tagged_and_round_trip() {
        let cases = [
            (Command::Ping, "ping"),
            (Command::ListDir { path: "src".into() }, "listDir"),
            (
                Command::GitCommit {
                    message: "wip".into(),
                    amend: false,
                },
                "gitCommit",
            ),
            (Command::GetWorkspaceSettings, "getWorkspaceSettings"),
        ];
        for (command, tag) in cases {
            let json = serde_json::to_value(&command).unwrap();
            assert_eq!(json["cmd"], tag);
            let round_tripped: Command = serde_json::from_value(json).unwrap();
            assert_eq!(round_tripped, command);
        }
    }

    #[test]
    fn a_frame_survives_a_line_of_json() {
        let frames = [
            Frame::Request {
                id: 7,
                command: Command::Hello {
                    protocol_version: PROTOCOL_VERSION,
                },
            },
            Frame::Response {
                id: 7,
                result: Some(serde_json::json!({ "ok": true })),
                error: None,
            },
            Frame::Response {
                id: 8,
                result: None,
                error: Some("boom".into()),
            },
            Frame::Event {
                event: EVENT_FS_CHANGES.into(),
                payload: serde_json::json!([]),
            },
        ];
        for frame in frames {
            let line = serde_json::to_string(&frame).unwrap();
            assert!(!line.contains('\n'), "frames must be one line: {line}");
            let round_tripped: Frame = serde_json::from_str(&line).unwrap();
            assert_eq!(round_tripped, frame);
        }
    }

    #[test]
    fn a_response_omits_the_side_it_did_not_use() {
        let ok = serde_json::to_value(Frame::Response {
            id: 1,
            result: Some(Value::Null),
            error: None,
        })
        .unwrap();
        assert!(ok.get("error").is_none());
        assert_eq!(ok["result"], Value::Null);
    }

    #[test]
    fn connection_targets_know_whether_they_are_remote() {
        assert!(!ConnectionTarget::Local.is_remote());
        assert!(ConnectionTarget::Wsl {
            distro: "Ubuntu".into()
        }
        .is_remote());
        assert_eq!(
            ConnectionTarget::Ssh { host: "box".into() }.label(),
            "SSH: box"
        );
    }

    #[test]
    fn a_hello_from_a_future_version_still_deserializes() {
        // Additive changes must not break the handshake — that is the whole
        // point of gating on `protocolVersion` rather than on parse success.
        let hello: Hello = serde_json::from_str(
            r#"{"name":"agentlens-daemon","version":"9.9.9","protocolVersion":2,"somethingNew":true}"#,
        )
        .unwrap();
        assert_eq!(hello.protocol_version, 2);
        assert!(hello.capabilities.is_empty());
    }

    #[test]
    fn watcher_status_serializes_with_camel_case_fields() {
        let status = WatcherStatus {
            state: WatcherState::Error,
            message: Some("boom".into()),
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["state"], "error");
        assert_eq!(json["message"], "boom");
    }
}
