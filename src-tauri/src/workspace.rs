//! Workspace open/close and recent-workspaces persistence.
//!
//! "Session" here is just *watching since T* — no snapshot/history logic
//! belongs in this module (that's a later slice).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::paths::normalize_absolute;

const RECENT_STORE_FILE: &str = "settings.json";
const RECENT_KEY: &str = "recentWorkspaces";
const MAX_RECENT: usize = 10;

/// The currently open workspace.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub name: String,
    pub watching_since: i64,
}

/// Tauri-managed state holding the (at most one) open workspace.
#[derive(Default)]
pub struct WorkspaceState(pub Mutex<Option<Workspace>>);

fn now_millis() -> i64 {
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

/// Record `root` as the most-recently opened workspace, deduplicated and
/// capped at `MAX_RECENT`.
pub fn record_recent(app: &AppHandle, root: &Path) -> Result<(), String> {
    let store = app
        .store(RECENT_STORE_FILE)
        .map_err(|e| format!("failed to open settings store: {e}"))?;

    let mut recent = read_recent_list(store.get(RECENT_KEY));
    let normalized = normalize_absolute(root);
    recent.retain(|p| p != &normalized);
    recent.insert(0, normalized);
    recent.truncate(MAX_RECENT);

    store.set(RECENT_KEY, json!(recent));
    store
        .save()
        .map_err(|e| format!("failed to save settings store: {e}"))?;

    Ok(())
}

/// Return the persisted list of recent workspace paths, most-recent-first.
pub fn recent_workspaces(app: &AppHandle) -> Result<Vec<String>, String> {
    let store = app
        .store(RECENT_STORE_FILE)
        .map_err(|e| format!("failed to open settings store: {e}"))?;
    Ok(read_recent_list(store.get(RECENT_KEY)))
}

fn read_recent_list(value: Option<serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default()
}
