//! Serializable types crossing the UI <-> backend boundary.
//!
//! Everything the frontend sends or receives is defined here and mirrored in
//! `src/lib/protocol.ts`. This is deliberate: a later phase replaces the
//! in-process backend with a remote daemon, and only serializable messages
//! survive that move.

use serde::{Deserialize, Serialize};

/// Identity of the running application, surfaced in the window title and UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

/// The result type for every `#[tauri::command]`, so the error shape is
/// uniform across the UI <-> backend boundary.
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
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            show_agent_context: true,
            agent_roots: Vec::new(),
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

/// Event names emitted by the backend. Rust `emit` calls and TS `listen`
/// calls must both go through these constants (mirrored in
/// `src/lib/protocol.ts`) so the names can't silently drift apart.
pub const EVENT_FS_CHANGES: &str = "fs-changes";
pub const EVENT_GIT_STATUS: &str = "git-status";
pub const EVENT_WATCHER_STATUS: &str = "watcher-status";

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
