//! The open workspace: which directory is being observed, and since when.
//!
//! "Session" here is just *watching since T* — the baselines that phrase
//! implies live in `snapshots`.
//!
//! Only the in-memory fact lives here. Which workspaces were opened *recently*
//! is a property of the person sitting in front of the app, not of the machine
//! holding the files, so that list stays with whoever is driving.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::paths::normalize_absolute;
use crate::protocol::WorkspaceInfo;

/// The currently open workspace.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub name: String,
    pub watching_since: i64,
}

impl Workspace {
    /// The protocol view of this workspace, with the root normalized for
    /// display.
    pub fn info(&self) -> WorkspaceInfo {
        WorkspaceInfo {
            root: normalize_absolute(&self.root),
            name: self.name.clone(),
            watching_since: self.watching_since,
        }
    }
}

/// Holds the (at most one) open workspace.
#[derive(Default)]
pub struct WorkspaceState(pub Mutex<Option<Workspace>>);

/// Unix epoch milliseconds, or 0 if the clock is before the epoch.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Open `path` as the workspace: canonicalize, verify it's a directory, and
/// replace whatever was previously open.
pub fn open(state: &WorkspaceState, path: &Path) -> Result<Workspace, String> {
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("failed to open workspace: {e}"))?;
    if !canonical.is_dir() {
        return Err("workspace path is not a directory".to_string());
    }
    let name = canonical
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| normalize_absolute(&canonical));

    let workspace = Workspace {
        root: canonical,
        name,
        watching_since: now_millis(),
    };

    let mut guard = state.0.lock().map_err(|_| "workspace state poisoned")?;
    *guard = Some(workspace.clone());
    Ok(workspace)
}

/// Reset the session clock to now, keeping the same workspace open. Paired
/// with re-capturing snapshot baselines, this is the "clear" action: it
/// redefines what "changed since the session started" means.
pub fn restart_session(state: &WorkspaceState) -> Result<Workspace, String> {
    let mut guard = state.0.lock().map_err(|_| "workspace state poisoned")?;
    let workspace = guard.as_mut().ok_or("no workspace is open")?;
    workspace.watching_since = now_millis();
    Ok(workspace.clone())
}

/// Close the currently open workspace, if any.
pub fn close(state: &WorkspaceState) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|_| "workspace state poisoned")?;
    *guard = None;
    Ok(())
}

/// Return a clone of the currently open workspace, or the uniform error
/// string used across commands when nothing is open.
pub fn current(state: &WorkspaceState) -> Result<Workspace, String> {
    let guard = state.0.lock().map_err(|_| "workspace state poisoned")?;
    guard
        .clone()
        .ok_or_else(|| "no workspace is open".to_string())
}

/// Return a clone of the currently open workspace, if any (does not error
/// when nothing is open).
pub fn current_opt(state: &WorkspaceState) -> Result<Option<Workspace>, String> {
    let guard = state.0.lock().map_err(|_| "workspace state poisoned")?;
    Ok(guard.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_a_directory_and_reports_it_as_current() {
        let dir = tempfile::tempdir().unwrap();
        let state = WorkspaceState::default();

        let opened = open(&state, dir.path()).unwrap();
        assert_eq!(
            opened.name,
            dir.path().file_name().unwrap().to_string_lossy()
        );
        assert!(opened.watching_since > 0);
        assert_eq!(current(&state).unwrap().root, opened.root);
    }

    #[test]
    fn refuses_a_file_and_a_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "x").unwrap();
        let state = WorkspaceState::default();

        assert!(open(&state, &file).is_err());
        assert!(open(&state, &dir.path().join("nope")).is_err());
    }

    #[test]
    fn closing_leaves_no_current_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let state = WorkspaceState::default();
        open(&state, dir.path()).unwrap();

        close(&state).unwrap();
        assert!(current_opt(&state).unwrap().is_none());
        assert!(current(&state).is_err());
    }

    #[test]
    fn restarting_the_session_keeps_the_root_and_moves_the_clock() {
        let dir = tempfile::tempdir().unwrap();
        let state = WorkspaceState::default();
        let opened = open(&state, dir.path()).unwrap();

        let restarted = restart_session(&state).unwrap();
        assert_eq!(restarted.root, opened.root);
        assert!(restarted.watching_since >= opened.watching_since);
    }
}
