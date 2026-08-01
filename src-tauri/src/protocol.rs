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
    /// True when the workspace isn't a git repository, which is what the
    /// session baseline is derived from.
    pub unavailable: bool,
}

/// Settings that apply to one workspace, persisted per root.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSettings {
    /// Extra gitignore-syntax globs, hidden from the tree, the file index,
    /// and the activity feed.
    pub extra_ignores: Vec<String>,
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
