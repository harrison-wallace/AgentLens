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
}
